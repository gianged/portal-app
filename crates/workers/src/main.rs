//! Thin entry point. Runs the tokio runtime on an 8 MiB thread because composing
//! the worker graph overflows Windows' 1 MiB main-thread stack in debug builds.
mod audit;
mod bootstrap;
mod cleanup;
mod config;
mod emails;
mod flex_reconciliation;
mod grpc;
mod job_error;
mod job_spool;
mod leave_expiry;
mod notifications;
mod report_schedule;
mod ticket_autoclose;
mod uploads;

#[cfg(not(unix))]
use std::future;
use std::{process, time::Duration};

use apalis::prelude::*;
#[cfg(unix)]
use tokio::signal::unix::{self, SignalKind};
use tokio::{signal, time};

use application::resilience;
use infrastructure::telemetry;

use crate::{bootstrap::WorkerContext, grpc::GrpcJobs, job_spool::JobSpoolDrainer};

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
    // Capture panics as structured logs, then stand up the log sinks; the guard keeps the file-writer flush thread alive.
    telemetry::install_panic_hook();
    let _log_guard = telemetry::init(&config::telemetry_config());

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
        leave,
        flex,
        email_queue,
        health_registry,
        health_checks,
        pg_breaker,
        chat_drainer,
        job_spool,
    } = bootstrap::build(&cfg).await?;

    // Internal gRPC ingest (+ standard health service), alongside the Monitor.
    // Shares the shutdown signal; a crash is logged, the direct apalis hop and
    // the job spool keep dispatch alive while it is down.
    {
        let jobs_service = GrpcJobs::new(
            storage.clone(),
            audit_storage.clone(),
            email_storage.clone(),
        );
        let addr = cfg.grpc_addr;
        let token = cfg.internal_grpc_token.clone();
        tokio::spawn(async move {
            if let Err(error) = grpc::serve(jobs_service, addr, &token).await {
                tracing::error!(%error, "workers grpc server failed");
            }
        });
    }

    // Replays job dispatches the server spooled while both enqueue hops were down.
    {
        let drainer = JobSpoolDrainer::new(
            job_spool,
            storage.clone(),
            audit_storage.clone(),
            email_storage.clone(),
        );
        resilience::supervise("job-spool-drainer", move || drainer.clone().run());
    }

    // Health prober drives the per-backend circuit breakers (fail-fast + recovery).
    {
        let registry = health_registry.clone();
        let checks = health_checks.clone();
        let interval = cfg.health_probe_interval;
        resilience::supervise("health-prober", move || {
            registry.clone().run_prober(checks.clone(), interval)
        });
    }

    // Periodic maintenance loops run alongside the queue consumer; supervised so a panic restarts the loop with backoff.
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

    // Daily leave-balance expiry sweep: warns on near-expiry grants and lapses expired ones (recording work % per policy).
    if cfg.leave_expiry_enabled {
        let leave = leave.clone();
        let interval = cfg.leave_expiry_interval;
        resilience::supervise("leave-expiry", move || {
            leave_expiry::run(leave.clone(), interval)
        });
    } else {
        tracing::info!("leave expiry sweep disabled (LEAVE_EXPIRY_ENABLED=false)");
        drop(leave);
    }

    // Month-end flex reconciliation: warns users whose approved flex hours don't net to the expected monthly total.
    if cfg.flex_recon_enabled {
        let flex = flex.clone();
        let interval = cfg.flex_recon_interval;
        resilience::supervise("flex-reconciliation", move || {
            flex_reconciliation::run(flex.clone(), interval)
        });
    } else {
        tracing::info!("flex reconciliation sweep disabled (FLEX_RECON_ENABLED=false)");
        drop(flex);
    }

    // One worker per durable queue; separate queues keep a notification retry from re-running the audit projector.
    // The Postgres breaker gates both PG-writing handlers, returning a retryable error while open.
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
        "workers ready: notification + audit + email consumers + maintenance loops (cleanup, uploads, ticket auto-close, monthly report, leave-expiry, flex-reconciliation)"
    );
    // Guarantees a shutdown signal exits even if the Monitor's graceful shutdown stalls.
    tokio::spawn(force_exit_watchdog());
    Monitor::new()
        .register(notify_worker)
        .register(audit_worker)
        .register(email_worker)
        .run_with_signal(async {
            wait_for_shutdown().await;
            Ok(())
        })
        .await?;

    tracing::info!("workers shut down");
    Ok(())
}

/// Force-exits [`FORCE_EXIT_GRACE`] after the signal so a shutdown reliably stops the process.
async fn force_exit_watchdog() {
    wait_for_shutdown().await;
    time::sleep(FORCE_EXIT_GRACE).await;
    process::exit(0);
}

/// Resolves on the first shutdown signal: Ctrl-C on every platform, plus
/// `SIGTERM` on Unix (the orchestrator's stop signal).
pub(crate) async fn wait_for_shutdown() {
    let ctrl_c = async {
        if let Err(error) = signal::ctrl_c().await {
            tracing::error!(%error, "failed to install Ctrl-C handler");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match unix::signal(SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::error!(%error, "failed to install SIGTERM handler"),
        }
    };
    #[cfg(not(unix))]
    let terminate = future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
