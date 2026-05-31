use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;

/// Runtime configuration, parsed once from the environment at startup. This is
/// the only place the server reads env vars; everything else receives a typed
/// `Config`.
///
/// `dead_code` is allowed because a few fields (`jwt_secret`, `session_ttl_secs`)
/// feed the auth middleware / login handler that land with the HTTP routes.
#[allow(dead_code)]
pub struct Config {
    pub database_url: String,
    pub pg_max_connections: u32,
    pub redis_url: String,
    pub scylla_hosts: Vec<String>,
    pub scylla_keyspace: String,
    pub openfga_api_url: String,
    pub openfga_model_path: PathBuf,
    pub openfga_bearer_token: Option<String>,
    pub storage_root: PathBuf,
    pub storage_public_base: String,
    pub server_addr: SocketAddr,
    pub jwt_secret: String,
    pub session_ttl_secs: u64,
    /// `Secure` attribute on the session cookie. Defaults to `true`; set
    /// `COOKIE_SECURE=false` for plain-HTTP local development.
    pub cookie_secure: bool,
}

pub fn from_env() -> anyhow::Result<Config> {
    let host = optional("SERVER_HOST", "0.0.0.0");
    let port = optional("SERVER_PORT", "8080");
    let server_addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .with_context(|| format!("invalid SERVER_HOST/SERVER_PORT: {host}:{port}"))?;

    let scylla_hosts = optional("SCYLLA_HOSTS", "127.0.0.1:9042")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let session_ttl_hours: u64 = optional("SESSION_TTL_HOURS", "24")
        .parse()
        .context("invalid SESSION_TTL_HOURS")?;

    Ok(Config {
        database_url: required("DATABASE_URL")?,
        pg_max_connections: optional("PG_MAX_CONNECTIONS", "10")
            .parse()
            .context("invalid PG_MAX_CONNECTIONS")?,
        redis_url: required("REDIS_URL")?,
        scylla_hosts,
        scylla_keyspace: optional("SCYLLA_KEYSPACE", "portal_chat"),
        openfga_api_url: required("OPENFGA_API_URL")?,
        openfga_model_path: optional(
            "OPENFGA_MODEL_PATH",
            "infra/openfga/authorization-model.json",
        )
        .into(),
        openfga_bearer_token: std::env::var("OPENFGA_BEARER_TOKEN")
            .ok()
            .filter(|s| !s.is_empty()),
        storage_root: optional("STORAGE_ROOT", "./storage/uploads").into(),
        storage_public_base: optional("STORAGE_PUBLIC_BASE", "http://localhost:8080/api/v1"),
        server_addr,
        jwt_secret: required("JWT_SECRET")?,
        session_ttl_secs: session_ttl_hours * 3600,
        cookie_secure: optional("COOKIE_SECURE", "true")
            .parse()
            .context("invalid COOKIE_SECURE (expected true/false)")?,
    })
}

fn required(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var {key}"))
}

fn optional(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
