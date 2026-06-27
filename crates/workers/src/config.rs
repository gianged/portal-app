use std::path::PathBuf;
use std::time::Duration as StdDuration;

use anyhow::Context;
use time::Duration;

/// Worker configuration, parsed once from the environment at startup. Adds the
/// maintenance-job intervals and retention windows on top of the server's backends.
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

    let email_enabled: bool = optional("EMAIL_ENABLED", "false")
        .parse()
        .context("invalid EMAIL_ENABLED (expected true/false)")?;
    let smtp_host = std::env::var("SMTP_HOST").ok().filter(|s| !s.is_empty());
    let smtp_from = std::env::var("SMTP_FROM").ok().filter(|s| !s.is_empty());
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
        ticket_autoclose_window: Duration::days(ticket_autoclose_days),
        ticket_autoclose_interval: StdDuration::from_secs(ticket_autoclose_interval_hours * 3600),
        email_enabled,
        smtp_host,
        smtp_port: optional("SMTP_PORT", "587")
            .parse()
            .context("invalid SMTP_PORT")?,
        smtp_username: std::env::var("SMTP_USERNAME")
            .ok()
            .filter(|s| !s.is_empty()),
        smtp_password: std::env::var("SMTP_PASSWORD")
            .ok()
            .filter(|s| !s.is_empty()),
        smtp_from,
        smtp_tls,
        portal_base_url: optional("PORTAL_BASE_URL", "http://localhost:8081"),
        report_enabled,
        report_schedule_day,
        report_schedule_interval: StdDuration::from_secs(report_schedule_interval_hours * 3600),
    })
}

fn required(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var {key}"))
}

fn optional(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
