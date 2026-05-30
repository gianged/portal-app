use anyhow::Context;

use infrastructure::{
    local_storage::LocalStorage, postgres::build_pool, redis::RedisEventPublisher,
    scylla::build_session,
};

use crate::config::Config;

/// Validates that workers can reach every backend they depend on, failing fast
/// at startup if any is misconfigured.
///
/// Job registration (an apalis `Monitor` plus the cleanup / notifications /
/// uploads handlers) lands separately and will hold these handles and the
/// application services; it also needs an apalis storage backend crate added to
/// `Cargo.toml`. For now this proves the composition wires up.
pub async fn connect(cfg: &Config) -> anyhow::Result<()> {
    let _pool = build_pool(&cfg.database_url, cfg.pg_max_connections)
        .await
        .context("building postgres pool")?;
    let _session = build_session(&cfg.scylla_hosts, &cfg.scylla_keyspace)
        .await
        .context("building scylla session")?;
    let _publisher = RedisEventPublisher::new(&cfg.redis_url)
        .await
        .context("connecting redis (events)")?;
    let _storage = LocalStorage::new(cfg.storage_root.clone(), cfg.storage_public_base.clone());
    Ok(())
}
