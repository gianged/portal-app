use std::{
    env,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};

use anyhow::Context;
use ipnet::IpNet;

use infrastructure::telemetry::{LogFormat, TelemetryConfig};

/// Telemetry settings, read separately so the log sinks stand up before the rest
/// of config is parsed (and config-parse errors are themselves logged). The
/// service name is fixed for this binary; `LOG_FORMAT` and the OTLP endpoint come
/// from the env. Never fails: a missing/blank var falls back to a default.
#[must_use]
pub fn telemetry_config() -> TelemetryConfig {
    TelemetryConfig {
        log_dir: PathBuf::from("logs"),
        file_prefix: "server".to_owned(),
        service_name: "portal-server".to_owned(),
        format: LogFormat::from_env(&optional("LOG_FORMAT", "tree")),
        otlp_endpoint: env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .ok()
            .filter(|s| !s.is_empty()),
    }
}

/// Runtime configuration, parsed once from the environment at startup. This is
/// the only place the server reads env vars; everything else receives a typed
/// `Config`.
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
    /// Bind address of the internal gRPC query plane.
    pub grpc_addr: SocketAddr,
    /// URL of the workers' internal gRPC ingest plane (job dispatch primary hop).
    pub workers_grpc_url: String,
    /// Shared bearer token gating both internal gRPC planes.
    pub internal_grpc_token: String,
    pub jwt_secret: String,
    /// HMAC key for presigned file-download URLs; deliberately distinct from
    /// `jwt_secret` so the two credentials can rotate independently.
    pub storage_signing_secret: String,
    pub session_ttl_secs: u64,
    /// `Secure` attribute on the session cookie. Defaults to `true`; set
    /// `COOKIE_SECURE=false` for plain-HTTP local development.
    pub cookie_secure: bool,
    /// Per-window request ceilings: `auth_rate_limit` gates `/login` per
    /// (client IP, email) pair; `auth_ip_rate_limit` gates the public auth routes
    /// per client IP and is deliberately loose because one office NAT can front
    /// hundreds of users; `api_rate_limit` the API and `chat_rate_limit` the WS
    /// `SendMessage` path per user (WS frames bypass the HTTP limiter);
    /// `rate_limit_window_secs` is the window.
    pub auth_rate_limit: u64,
    pub auth_ip_rate_limit: u64,
    pub api_rate_limit: u64,
    pub chat_rate_limit: u64,
    /// Ceiling for the external read API, per service-account key.
    pub ext_rate_limit: u64,
    /// Per-IP ceiling for the external read API, applied before key auth.
    pub ext_ip_rate_limit: u64,
    pub rate_limit_window_secs: i64,
    /// Origins allowed to call the API with credentials (the WASM frontend).
    /// Credentialed CORS forbids a wildcard, so these are enumerated.
    pub cors_allowed_origins: Vec<String>,
    /// How often the health prober pings each backend to drive its breaker and
    /// the `/readyz` snapshot.
    pub health_probe_interval: std::time::Duration,
    /// Network gate: when `true`, only clients whose peer IP falls in
    /// `ip_allowlist` reach the API; others get 403. Toggle via
    /// `IP_ALLOWLIST_ENABLED` (default on).
    pub ip_allowlist_enabled: bool,
    /// Allowed source networks (CIDR). Defaults to loopback + RFC1918/ULA private
    /// ranges so LAN and VPN clients pass; override with `IP_ALLOWLIST`.
    pub ip_allowlist: Vec<IpNet>,
    /// Reverse proxies whose `X-Forwarded-For` is trusted (CIDR). When the peer
    /// matches, the gate judges the forwarded client instead of the peer. Empty
    /// (the default) means the header is never trusted. Set via `TRUSTED_PROXIES`.
    pub trusted_proxies: Vec<IpNet>,
}

pub fn from_env() -> anyhow::Result<Config> {
    let host = optional("SERVER_HOST", "0.0.0.0");
    let port = optional("SERVER_PORT", "8080");
    let server_addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .with_context(|| format!("invalid SERVER_HOST/SERVER_PORT: {host}:{port}"))?;
    let grpc_addr: SocketAddr = optional("SERVER_GRPC_ADDR", "0.0.0.0:50051")
        .parse()
        .context("invalid SERVER_GRPC_ADDR")?;

    // Without a bearer token the OpenFGA API accepts unauthenticated writes to
    // the entire authorization graph, so its absence must be an explicit,
    // dev-only opt-in rather than a silent default.
    let openfga_bearer_token = env::var("OPENFGA_BEARER_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    let openfga_allow_no_auth: bool = optional("OPENFGA_ALLOW_NO_AUTH", "false")
        .parse()
        .context("invalid OPENFGA_ALLOW_NO_AUTH (expected true/false)")?;
    if openfga_bearer_token.is_none() && !openfga_allow_no_auth {
        anyhow::bail!(
            "OPENFGA_BEARER_TOKEN is required (or set OPENFGA_ALLOW_NO_AUTH=true for local dev \
             where OpenFGA runs without authn)"
        );
    }

    let scylla_hosts = optional("SCYLLA_HOSTS", "127.0.0.1:9042")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let session_ttl_hours: u64 = optional("SESSION_TTL_HOURS", "24")
        .parse()
        .context("invalid SESSION_TTL_HOURS")?;

    let cors_allowed_origins = optional("CORS_ALLOWED_ORIGINS", "http://localhost:8080")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let ip_allowlist_enabled: bool = optional("IP_ALLOWLIST_ENABLED", "true")
        .parse()
        .context("invalid IP_ALLOWLIST_ENABLED (expected true/false)")?;
    let ip_allowlist = parse_allowlist("IP_ALLOWLIST", DEFAULT_IP_ALLOWLIST)?;
    let trusted_proxies = parse_allowlist("TRUSTED_PROXIES", "")?;

    Ok(Config {
        database_url: required("DATABASE_URL")?,
        pg_max_connections: optional("PG_MAX_CONNECTIONS", "32")
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
        openfga_bearer_token,
        storage_root: optional("STORAGE_ROOT", "./storage/uploads").into(),
        storage_public_base: optional("STORAGE_PUBLIC_BASE", "http://localhost:8080/api/v1"),
        server_addr,
        grpc_addr,
        workers_grpc_url: optional("WORKERS_GRPC_URL", "http://127.0.0.1:50052"),
        internal_grpc_token: required_secret("INTERNAL_GRPC_TOKEN")?,
        jwt_secret: required_secret("JWT_SECRET")?,
        storage_signing_secret: required_secret("STORAGE_SIGNING_SECRET")?,
        session_ttl_secs: session_ttl_hours * 3600,
        cookie_secure: optional("COOKIE_SECURE", "true")
            .parse()
            .context("invalid COOKIE_SECURE (expected true/false)")?,
        auth_rate_limit: optional("AUTH_RATE_LIMIT", "10")
            .parse()
            .context("invalid AUTH_RATE_LIMIT")?,
        auth_ip_rate_limit: optional("AUTH_IP_RATE_LIMIT", "600")
            .parse()
            .context("invalid AUTH_IP_RATE_LIMIT")?,
        api_rate_limit: optional("API_RATE_LIMIT", "120")
            .parse()
            .context("invalid API_RATE_LIMIT")?,
        chat_rate_limit: optional("CHAT_RATE_LIMIT", "120")
            .parse()
            .context("invalid CHAT_RATE_LIMIT")?,
        ext_rate_limit: optional("EXT_RATE_LIMIT", "60")
            .parse()
            .context("invalid EXT_RATE_LIMIT")?,
        ext_ip_rate_limit: optional("EXT_IP_RATE_LIMIT", "120")
            .parse()
            .context("invalid EXT_IP_RATE_LIMIT")?,
        rate_limit_window_secs: optional("RATE_LIMIT_WINDOW_SECS", "60")
            .parse()
            .context("invalid RATE_LIMIT_WINDOW_SECS")?,
        cors_allowed_origins,
        health_probe_interval: std::time::Duration::from_secs(
            optional("HEALTH_PROBE_INTERVAL_SECS", "5")
                .parse()
                .context("invalid HEALTH_PROBE_INTERVAL_SECS")?,
        ),
        ip_allowlist_enabled,
        ip_allowlist,
        trusted_proxies,
    })
}

/// Default allowlist: loopback plus RFC1918 (v4) and ULA (v6) private ranges, so a
/// deploy inside the corporate LAN and its VPN clients pass without configuration.
const DEFAULT_IP_ALLOWLIST: &str =
    "127.0.0.0/8,::1/128,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,fc00::/7";

/// Reads `var` as a comma-separated network list. Each token is a CIDR (`10.0.0.0/8`)
/// or a bare address promoted to a host route (`/32` or `/128`). A malformed token
/// fails startup rather than silently narrowing the gate.
fn parse_allowlist(var: &str, default: &str) -> anyhow::Result<Vec<IpNet>> {
    optional(var, default)
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|tok| {
            IpNet::from_str(tok)
                .or_else(|_| IpAddr::from_str(tok).map(IpNet::from))
                .with_context(|| format!("invalid {var} entry: {tok}"))
        })
        .collect()
}

fn required(key: &str) -> anyhow::Result<String> {
    env::var(key).with_context(|| format!("missing required env var {key}"))
}

/// [`required`] for secrets: rejects values short enough to brute-force and the
/// `.env.example` placeholders, so a copied example file fails fast instead of
/// shipping a publicly-known signing key.
fn required_secret(key: &str) -> anyhow::Result<String> {
    let value = required(key)?;
    if value.len() < 32 || value.starts_with("change-me") {
        anyhow::bail!(
            "{key} must be a random secret of at least 32 bytes — generate one with \
             `openssl rand -hex 32`"
        );
    }
    Ok(value)
}

fn optional(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}
