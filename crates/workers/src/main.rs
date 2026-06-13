//! Thin entry point. Like `server::main`, runs the tokio runtime on an 8 MiB
//! thread: composing the worker graph overflows Windows' 1 MiB main-thread stack
//! in debug builds (`STATUS_STACK_OVERFLOW`).
mod audit;
mod bootstrap;
mod cleanup;
mod config;
mod emails;
mod notifications;
mod telemetry;
mod ticket_autoclose;
mod uploads;

use std::time::Duration;

use apalis::prelude::*;

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
    telemetry::init();

    let cfg = config::from_env()?;
    let WorkerContext {
        fanout,
        storage,
        audit_projector,
        audit_storage,
        maintenance,
        mailer,
        email_storage,
    } = bootstrap::build(&cfg).await?;

    // Periodic maintenance loops run alongside the queue consumer. Each loop handles and
    // logs its own errors internally and never returns, so dropping the `JoinHandle` only
    // forgoes observing an outright panic (a bug). They are idempotent, so aborting them
    // when the runtime stops at shutdown is safe.
    tokio::spawn(cleanup::run(
        maintenance.clone(),
        cfg.notification_retention,
        cfg.cleanup_interval,
    ));
    tokio::spawn(uploads::run(
        maintenance.clone(),
        cfg.upload_grace,
        cfg.upload_sweep_interval,
    ));
    tokio::spawn(ticket_autoclose::run(
        maintenance,
        cfg.ticket_autoclose_window,
        cfg.ticket_autoclose_interval,
    ));

    // One worker per durable queue the server enqueues. Separate queues keep the
    // (non-idempotent) notification fan-out and the audit projector isolated so a
    // retry in one never re-runs the other.
    let notify_worker = WorkerBuilder::new("notifications")
        .data(fanout)
        .backend(storage)
        .build_fn(notifications::handle);
    let audit_worker = WorkerBuilder::new("audit")
        .data(audit_projector)
        .backend(audit_storage)
        .build_fn(audit::handle);
    let email_worker = WorkerBuilder::new("emails")
        .data(mailer)
        .backend(email_storage)
        .build_fn(emails::handle);

    tracing::info!(
        "workers ready: notification + audit + email consumers + maintenance loops (cleanup, uploads, ticket auto-close)"
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

/// Force-exits [`FORCE_EXIT_GRACE`] after the signal if shutdown hasn't finished,
/// so Ctrl-C reliably stops the process.
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
