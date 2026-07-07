use std::{env, path::PathBuf, time::Duration as StdDuration};

use anyhow::Context;
use time::Duration;

use infrastructure::telemetry::{LogFormat, TelemetryConfig};

/// Telemetry settings, read separately so the log sinks stand up before the rest
/// of config is parsed.
#[must_use]
pub fn telemetry_config() -> TelemetryConfig {
    TelemetryConfig {
        log_dir: PathBuf::from("logs"),
        file_prefix: "workers".to_owned(),
        service_name: "portal-workers".to_owned(),
        format: LogFormat::from_env(&optional("LOG_FORMAT", "tree")),
        otlp_endpoint: env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .ok()
            .filter(|s| !s.is_empty()),
    }
}

/// Worker configuration, parsed once from the environment at startup. Adds the
/// maintenance-job intervals and retention windows on top of the server's backends.
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
    /// HMAC key for signing storage download URLs.
    pub storage_signing_secret: String,
    /// Read notifications older than this are pruned.
    pub notification_retention: Duration,
    /// How often the notification-prune job runs.
    pub cleanup_interval: StdDuration,
    /// Upload objects untouched for at least this long are sweep-eligible.
    pub upload_grace: Duration,
    /// How often the orphan-upload sweep runs.
    pub upload_sweep_interval: StdDuration,
    /// Resolved tickets older than this are auto-closed (the reopen window).
    pub ticket_autoclose_window: Duration,
    /// How often the ticket auto-close sweep runs.
    pub ticket_autoclose_interval: StdDuration,
    /// When false (the dev default) emails are logged, not sent, and no SMTP
    /// settings are needed.
    pub email_enabled: bool,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    /// From address; required when email is enabled.
    pub smtp_from: Option<String>,
    /// `starttls` (default) or `none` for plain in-network relays.
    pub smtp_tls: String,
    /// Public frontend origin used for the links inside emails.
    pub portal_base_url: String,
    /// When false, the monthly report scheduler does not run.
    pub report_enabled: bool,
    /// Day of month on/after which the previous month's report is generated.
    pub report_schedule_day: u8,
    /// How often the report scheduler wakes to check whether it should run.
    pub report_schedule_interval: StdDuration,
    /// How often the health prober pings each backend to drive its breaker.
    pub health_probe_interval: StdDuration,
    /// When false, the daily leave-expiry sweep does not run.
    pub leave_expiry_enabled: bool,
    /// How often the leave-expiry sweep runs.
    pub leave_expiry_interval: StdDuration,
    /// When false, the month-end flex reconciliation sweep does not run.
    pub flex_recon_enabled: bool,
    /// How often the flex reconciliation sweep wakes to check the date.
    pub flex_recon_interval: StdDuration,
}

/// Parses worker configuration from the process environment.
///
/// # Errors
/// Returns an error if a required var is missing or any value fails to parse.
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
    // Default mirrors the domain's 7-day reopen window (Ticket::reopen).
    let ticket_autoclose_days: i64 = optional("TICKET_AUTOCLOSE_DAYS", "7")
        .parse()
        .context("invalid TICKET_AUTOCLOSE_DAYS")?;
    let ticket_autoclose_interval_hours: u64 = optional("TICKET_AUTOCLOSE_INTERVAL_HOURS", "1")
        .parse()
        .context("invalid TICKET_AUTOCLOSE_INTERVAL_HOURS")?;

    let report_enabled: bool = optional("REPORT_ENABLED", "true")
        .parse()
        .context("invalid REPORT_ENABLED (expected true/false)")?;
    let report_schedule_day: u8 = optional("REPORT_SCHEDULE_DAY", "1")
        .parse()
        .context("invalid REPORT_SCHEDULE_DAY")?;
    let report_schedule_interval_hours: u64 = optional("REPORT_SCHEDULE_INTERVAL_HOURS", "24")
        .parse()
        .context("invalid REPORT_SCHEDULE_INTERVAL_HOURS")?;
    let health_probe_interval_secs: u64 = optional("HEALTH_PROBE_INTERVAL_SECS", "5")
        .parse()
        .context("invalid HEALTH_PROBE_INTERVAL_SECS")?;

    // OpenFGA wiring for the leave service; mirrors the server's store/model resolution.
    let openfga_bearer_token = env::var("OPENFGA_BEARER_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    let openfga_allow_no_auth: bool = optional("OPENFGA_ALLOW_NO_AUTH", "false")
        .parse()
        .context("invalid OPENFGA_ALLOW_NO_AUTH (expected true/false)")?;
    if openfga_bearer_token.is_none() && !openfga_allow_no_auth {
        anyhow::bail!(
            "OPENFGA_BEARER_TOKEN is required (or set OPENFGA_ALLOW_NO_AUTH=true for local dev)"
        );
    }

    let leave_expiry_enabled: bool = optional("LEAVE_EXPIRY_ENABLED", "true")
        .parse()
        .context("invalid LEAVE_EXPIRY_ENABLED (expected true/false)")?;
    let leave_expiry_interval_hours: u64 = optional("LEAVE_EXPIRY_INTERVAL_HOURS", "24")
        .parse()
        .context("invalid LEAVE_EXPIRY_INTERVAL_HOURS")?;

    let flex_recon_enabled: bool = optional("FLEX_RECON_ENABLED", "true")
        .parse()
        .context("invalid FLEX_RECON_ENABLED (expected true/false)")?;
    let flex_recon_interval_hours: u64 = optional("FLEX_RECON_INTERVAL_HOURS", "24")
        .parse()
        .context("invalid FLEX_RECON_INTERVAL_HOURS")?;

    let email_enabled: bool = optional("EMAIL_ENABLED", "false")
        .parse()
        .context("invalid EMAIL_ENABLED (expected true/false)")?;
    let smtp_host = env::var("SMTP_HOST").ok().filter(|s| !s.is_empty());
    let smtp_from = env::var("SMTP_FROM").ok().filter(|s| !s.is_empty());
    let smtp_tls = optional("SMTP_TLS", "starttls");
    if !matches!(smtp_tls.as_str(), "starttls" | "none") {
        anyhow::bail!("invalid SMTP_TLS (expected starttls or none)");
    }
    if email_enabled && smtp_host.is_none() {
        anyhow::bail!("EMAIL_ENABLED=true requires SMTP_HOST");
    }
    if email_enabled && smtp_from.is_none() {
        anyhow::bail!("EMAIL_ENABLED=true requires SMTP_FROM");
    }

    Ok(Config {
        database_url: required("DATABASE_URL")?,
        pg_max_connections: optional("PG_MAX_CONNECTIONS", "16")
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
        storage_signing_secret: required_secret("STORAGE_SIGNING_SECRET")?,
        notification_retention: Duration::days(retention_days),
        cleanup_interval: StdDuration::from_secs(cleanup_interval_hours * 3600),
        upload_grace: Duration::hours(upload_grace_hours),
        upload_sweep_interval: StdDuration::from_secs(upload_sweep_interval_hours * 3600),
        ticket_autoclose_window: Duration::days(ticket_autoclose_days),
        ticket_autoclose_interval: StdDuration::from_secs(ticket_autoclose_interval_hours * 3600),
        email_enabled,
        smtp_host,
        smtp_port: optional("SMTP_PORT", "587")
            .parse()
            .context("invalid SMTP_PORT")?,
        smtp_username: env::var("SMTP_USERNAME").ok().filter(|s| !s.is_empty()),
        smtp_password: env::var("SMTP_PASSWORD").ok().filter(|s| !s.is_empty()),
        smtp_from,
        smtp_tls,
        portal_base_url: optional("PORTAL_BASE_URL", "http://localhost:8081"),
        report_enabled,
        report_schedule_day,
        report_schedule_interval: StdDuration::from_secs(report_schedule_interval_hours * 3600),
        health_probe_interval: StdDuration::from_secs(health_probe_interval_secs),
        leave_expiry_enabled,
        leave_expiry_interval: StdDuration::from_secs(leave_expiry_interval_hours * 3600),
        flex_recon_enabled,
        flex_recon_interval: StdDuration::from_secs(flex_recon_interval_hours * 3600),
    })
}

fn required(key: &str) -> anyhow::Result<String> {
    env::var(key).with_context(|| format!("missing required env var {key}"))
}

fn optional(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Like [`required`] but rejects brute-forceable values and `.env.example` placeholders.
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
