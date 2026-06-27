//! Thin entry point. Runs the tokio runtime on an 8 MiB thread because composing
//! the worker graph overflows Windows' 1 MiB main-thread stack in debug builds.
mod audit;
mod bootstrap;
mod cleanup;
mod config;
mod emails;
mod notifications;
mod report_schedule;
mod ticket_autoclose;
mod uploads;

use std::{path::Path, time::Duration};

use apalis::prelude::*;
use application::resilience;

use crate::bootstrap::WorkerContext;

/// Grace after the shutdown signal before the watchdog forces exit.
const FORCE_EXIT_GRACE: Duration = Duration::from_secs(3);

fn main() -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("workers-main".to_owned())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(run())
        })?
        .join()
        .expect("workers thread panicked")
}

async fn run() -> anyhow::Result<()> {
    // Populate the process env from the repo-root .env before config is parsed.
    dotenvy::dotenv().ok();
    // Capture panics as structured logs, then stand up the log sinks. The guard
    // keeps the file-writer flush thread alive for the process lifetime.
    infrastructure::telemetry::install_panic_hook();
    let _log_guard = infrastructure::telemetry::init(Path::new("logs"), "workers");

    let cfg = config::from_env()?;
    let WorkerContext {
        fanout,
        storage,
        audit_projector,
        audit_storage,
        maintenance,
        mailer,
        email_storage,
        report,
        email_queue,
        health_registry,
        health_checks,
        pg_breaker,
        chat_drainer,
    } = bootstrap::build(&cfg).await?;

    // Health prober drives the per-backend circuit breakers (fail-fast + recovery).
    {
        let registry = health_registry.clone();
        let checks = health_checks.clone();
        let interval = cfg.health_probe_interval;
        resilience::supervise("health-prober", move || {
            registry.clone().run_prober(checks.clone(), interval)
        });
    }

    // Periodic maintenance loops run alongside the queue consumer. Each handles its own
    // errors and never returns; supervised so a panic restarts the loop with backoff.
    {
        let m = maintenance.clone();
        let (retention, interval) = (cfg.notification_retention, cfg.cleanup_interval);
        resilience::supervise("cleanup", move || {
            cleanup::run(m.clone(), retention, interval)
        });
    }
    {
        let m = maintenance.clone();
        let (grace, interval) = (cfg.upload_grace, cfg.upload_sweep_interval);
        resilience::supervise("uploads", move || uploads::run(m.clone(), grace, interval));
    }
    {
        let m = maintenance.clone();
        let (window, interval) = (cfg.ticket_autoclose_window, cfg.ticket_autoclose_interval);
        resilience::supervise("ticket-autoclose", move || {
            ticket_autoclose::run(m.clone(), window, interval)
        });
    }
    // Chat spool drainer: replays optimistically-acked batches that couldn't reach
    // Scylla, paced by the Scylla breaker so a revived backend isn't flooded.
    resilience::supervise("chat-drainer", move || chat_drainer.clone().run());

    if cfg.report_enabled {
        let reports = report.clone();
        let queue = email_queue.clone();
        let (day, interval) = (cfg.report_schedule_day, cfg.report_schedule_interval);
        resilience::supervise("report-schedule", move || {
            report_schedule::run(reports.clone(), queue.clone(), day, interval)
        });
    } else {
        tracing::info!("monthly report scheduler disabled (REPORT_ENABLED=false)");
        drop((report, email_queue));
    }

    // One worker per durable queue. Separate queues isolate the non-idempotent
    // notification fan-out from the audit projector so a retry never re-runs the other.
    // The Postgres breaker gates the two PG-writing handlers: while it is open they
    // return a retryable error so the job stays queued, paced by the breaker cooldown.
    let notify_worker = WorkerBuilder::new("notifications")
        .data(fanout)
        .data(pg_breaker.clone())
        .backend(storage)
        .build_fn(notifications::handle);
    let audit_worker = WorkerBuilder::new("audit")
        .data(audit_projector)
        .data(pg_breaker)
        .backend(audit_storage)
        .build_fn(audit::handle);
    let email_worker = WorkerBuilder::new("emails")
        .data(mailer)
        .backend(email_storage)
        .build_fn(emails::handle);

    tracing::info!(
        "workers ready: notification + audit + email consumers + maintenance loops (cleanup, uploads, ticket auto-close, monthly report)"
    );
    // Guarantees Ctrl-C exits even if the Monitor's graceful shutdown stalls.
    tokio::spawn(force_exit_watchdog());
    Monitor::new()
        .register(notify_worker)
        .register(audit_worker)
        .register(email_worker)
        .run_with_signal(tokio::signal::ctrl_c())
        .await?;

    tracing::info!("workers shut down");
    Ok(())
}

/// Force-exits [`FORCE_EXIT_GRACE`] after the signal so Ctrl-C reliably stops the process.
async fn force_exit_watchdog() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tokio::time::sleep(FORCE_EXIT_GRACE).await;
    std::process::exit(0);
}
