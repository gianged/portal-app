use std::sync::Arc;

use anyhow::Context;
use apalis_redis::RedisStorage;

use application::{AuditProjector, MaintenanceService, NotificationFanout};
use domain::{
    ports::file_storage::FileStorage,
    repository::{
        AuditRepository, ChatRepository, GroupRepository, NotificationRepository,
        ProjectRepository, RequestRepository, TicketRepository, UserRepository,
    },
};
use infrastructure::{
    jobs::{AuditEnvelope, NotificationEnvelope, audit_storage, notification_storage},
    local_storage::LocalStorage,
    postgres::{
        PgAuditRepo, PgGroupRepo, PgNotificationRepo, PgProjectRepo, PgRequestRepo, PgTicketRepo,
        PgUserRepo, build_pool,
    },
    scylla::{ScyllaChatRepo, build_session},
    signed_url::SignedUrl,
};

use crate::config::Config;

/// Everything the job handlers need, assembled once at startup.
pub struct WorkerContext {
    pub fanout: Arc<NotificationFanout>,
    pub storage: RedisStorage<NotificationEnvelope>,
    pub audit_projector: Arc<AuditProjector>,
    pub audit_storage: RedisStorage<AuditEnvelope>,
    pub maintenance: Arc<MaintenanceService>,
}

/// Builds the infrastructure adapters, wires the notification fan-out service,
/// and opens the apalis job storage the worker consumes. Mirrors the server
/// composition root, minus the HTTP/authz slice the fan-out does not need
/// (fan-out runs as the system, so no `Permissions`/`OpenFGA`).
pub async fn build(cfg: &Config) -> anyhow::Result<WorkerContext> {
    let pool = build_pool(&cfg.database_url, cfg.pg_max_connections)
        .await
        .context("building postgres pool")?;
    let session = build_session(&cfg.scylla_hosts, &cfg.scylla_keyspace)
        .await
        .context("building scylla session")?;

    let notifications: Arc<dyn NotificationRepository> =
        Arc::new(PgNotificationRepo::new(pool.clone()));
    let groups: Arc<dyn GroupRepository> = Arc::new(PgGroupRepo::new(pool.clone()));
    let users: Arc<dyn UserRepository> = Arc::new(PgUserRepo::new(pool.clone()));
    let requests: Arc<dyn RequestRepository> = Arc::new(PgRequestRepo::new(pool.clone()));
    let chats: Arc<dyn ChatRepository> = Arc::new(
        ScyllaChatRepo::new(session)
            .await
            .context("preparing scylla statements")?,
    );
    let tickets: Arc<dyn TicketRepository> = Arc::new(PgTicketRepo::new(pool.clone()));
    let projects: Arc<dyn ProjectRepository> = Arc::new(PgProjectRepo::new(pool.clone()));
    // The orphan-sweep job only lists and deletes; it never presigns, so this
    // signer is never exercised (hence the empty key).
    let signer = Arc::new(SignedUrl::new(b""));
    let file_storage: Arc<dyn FileStorage> = Arc::new(LocalStorage::new(
        cfg.storage_root.clone(),
        &cfg.storage_public_base,
        signer,
    ));

    // Built before the fan-out moves the repo handles below.
    let maintenance = Arc::new(MaintenanceService::new(
        notifications.clone(),
        requests.clone(),
        users.clone(),
        file_storage,
    ));

    let audit: Arc<dyn AuditRepository> = Arc::new(PgAuditRepo::new(pool.clone()));
    let audit_projector = Arc::new(AuditProjector::new(audit));

    let fanout = Arc::new(NotificationFanout::new(
        notifications,
        groups,
        users,
        requests,
        chats,
        tickets,
        projects,
    ));

    let storage = notification_storage(&cfg.redis_url)
        .await
        .context("connecting apalis redis (jobs)")?;
    let audit_storage = audit_storage(&cfg.redis_url)
        .await
        .context("connecting apalis redis (audit jobs)")?;

    Ok(WorkerContext {
        fanout,
        storage,
        audit_projector,
        audit_storage,
        maintenance,
    })
}
