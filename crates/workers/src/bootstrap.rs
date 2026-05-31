use std::sync::Arc;

use anyhow::Context;
use apalis_redis::RedisStorage;

use application::{MaintenanceService, NotificationFanout};
use domain::{
    ports::file_storage::FileStorage,
    repository::{
        ChatRepository, GroupRepository, NotificationRepository, RequestRepository, UserRepository,
    },
};
use infrastructure::{
    jobs::{NotificationEnvelope, notification_storage},
    local_storage::LocalStorage,
    postgres::{PgGroupRepo, PgNotificationRepo, PgRequestRepo, PgUserRepo, build_pool},
    scylla::{ScyllaChatRepo, build_session},
};

use crate::config::Config;

/// Everything the job handlers need, assembled once at startup.
pub struct WorkerContext {
    pub fanout: Arc<NotificationFanout>,
    pub storage: RedisStorage<NotificationEnvelope>,
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
    let file_storage: Arc<dyn FileStorage> = Arc::new(LocalStorage::new(
        cfg.storage_root.clone(),
        &cfg.storage_public_base,
    ));

    // Built before the fan-out moves the repo handles below.
    let maintenance = Arc::new(MaintenanceService::new(
        notifications.clone(),
        requests.clone(),
        users.clone(),
        file_storage,
    ));

    let fanout = Arc::new(NotificationFanout::new(
        notifications,
        groups,
        users,
        requests,
        chats,
    ));

    let storage = notification_storage(&cfg.redis_url)
        .await
        .context("connecting apalis redis (jobs)")?;

    Ok(WorkerContext {
        fanout,
        storage,
        maintenance,
    })
}
