mod audit;
mod bootstrap;
mod cleanup;
mod config;
mod notifications;
mod telemetry;
mod uploads;

use apalis::prelude::*;

use crate::bootstrap::WorkerContext;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
        maintenance,
        cfg.upload_grace,
        cfg.upload_sweep_interval,
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

    tracing::info!("workers ready: notification + audit consumers + maintenance loops");
    Monitor::new()
        .register(notify_worker)
        .register(audit_worker)
        .run_with_signal(tokio::signal::ctrl_c())
        .await?;

    tracing::info!("workers shut down");
    Ok(())
}
