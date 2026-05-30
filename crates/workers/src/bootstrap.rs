use std::sync::Arc;

use anyhow::Context;
use apalis_redis::RedisStorage;

use application::NotificationFanout;
use domain::repository::{
    ChatRepository, GroupRepository, NotificationRepository, RequestRepository, UserRepository,
};
use infrastructure::{
    jobs::{NotificationEnvelope, notification_storage},
    postgres::{PgGroupRepo, PgNotificationRepo, PgRequestRepo, PgUserRepo, build_pool},
    scylla::{ScyllaChatRepo, build_session},
};

use crate::config::Config;

/// Everything the job handlers need, assembled once at startup.
pub struct WorkerContext {
    pub fanout: Arc<NotificationFanout>,
    pub storage: RedisStorage<NotificationEnvelope>,
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

    Ok(WorkerContext { fanout, storage })
}
