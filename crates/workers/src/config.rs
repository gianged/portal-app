use std::path::PathBuf;
use std::time::Duration as StdDuration;

use anyhow::Context;
use time::Duration;

/// Worker configuration, parsed once from the environment at startup. Workers
/// share the server's backends and add the maintenance-job intervals + retention
/// windows.
pub struct Config {
    pub database_url: String,
    pub pg_max_connections: u32,
    pub redis_url: String,
    pub scylla_hosts: Vec<String>,
    pub scylla_keyspace: String,
    pub storage_root: PathBuf,
    pub storage_public_base: String,
    /// Read notifications older than this are pruned.
    pub notification_retention: Duration,
    /// How often the notification-prune job runs.
    pub cleanup_interval: StdDuration,
    /// Upload objects untouched for at least this long are sweep-eligible.
    pub upload_grace: Duration,
    /// How often the orphan-upload sweep runs.
    pub upload_sweep_interval: StdDuration,
}

pub fn from_env() -> anyhow::Result<Config> {
    let scylla_hosts = optional("SCYLLA_HOSTS", "127.0.0.1:9042")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let retention_days: i64 = optional("NOTIFICATION_RETENTION_DAYS", "30")
        .parse()
        .context("invalid NOTIFICATION_RETENTION_DAYS")?;
    let cleanup_interval_hours: u64 = optional("CLEANUP_INTERVAL_HOURS", "24")
        .parse()
        .context("invalid CLEANUP_INTERVAL_HOURS")?;
    let upload_grace_hours: i64 = optional("UPLOAD_ORPHAN_GRACE_HOURS", "24")
        .parse()
        .context("invalid UPLOAD_ORPHAN_GRACE_HOURS")?;
    let upload_sweep_interval_hours: u64 = optional("UPLOAD_SWEEP_INTERVAL_HOURS", "6")
        .parse()
        .context("invalid UPLOAD_SWEEP_INTERVAL_HOURS")?;

    Ok(Config {
        database_url: required("DATABASE_URL")?,
        pg_max_connections: optional("PG_MAX_CONNECTIONS", "10")
            .parse()
            .context("invalid PG_MAX_CONNECTIONS")?,
        redis_url: required("REDIS_URL")?,
        scylla_hosts,
        scylla_keyspace: optional("SCYLLA_KEYSPACE", "portal_chat"),
        storage_root: optional("STORAGE_ROOT", "./storage/uploads").into(),
        storage_public_base: optional("STORAGE_PUBLIC_BASE", "http://localhost:8080/api/v1"),
        notification_retention: Duration::days(retention_days),
        cleanup_interval: StdDuration::from_secs(cleanup_interval_hours * 3600),
        upload_grace: Duration::hours(upload_grace_hours),
        upload_sweep_interval: StdDuration::from_secs(upload_sweep_interval_hours * 3600),
    })
}

fn required(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var {key}"))
}

fn optional(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
