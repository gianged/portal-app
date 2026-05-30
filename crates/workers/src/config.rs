use std::path::PathBuf;

use anyhow::Context;

/// Worker configuration, parsed once from the environment at startup. Workers
/// share the same backends as the server but need no HTTP/auth settings.
pub struct Config {
    pub database_url: String,
    pub pg_max_connections: u32,
    pub redis_url: String,
    pub scylla_hosts: Vec<String>,
    pub scylla_keyspace: String,
    pub storage_root: PathBuf,
    pub storage_public_base: String,
}

pub fn from_env() -> anyhow::Result<Config> {
    let scylla_hosts = optional("SCYLLA_HOSTS", "127.0.0.1:9042")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(Config {
        database_url: required("DATABASE_URL")?,
        pg_max_connections: optional("PG_MAX_CONNECTIONS", "10")
            .parse()
            .context("invalid PG_MAX_CONNECTIONS")?,
        redis_url: required("REDIS_URL")?,
        scylla_hosts,
        scylla_keyspace: optional("SCYLLA_KEYSPACE", "portal_chat"),
        storage_root: optional("STORAGE_ROOT", "./storage/uploads").into(),
        storage_public_base: optional("STORAGE_PUBLIC_BASE", "http://localhost:8080"),
    })
}

fn required(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var {key}"))
}

fn optional(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
