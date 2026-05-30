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
    let WorkerContext { fanout, storage } = bootstrap::build(&cfg).await?;

    // One worker consuming the durable `notifications` queue the server enqueues.
    let worker = WorkerBuilder::new("notifications")
        .data(fanout)
        .backend(storage)
        .build_fn(notifications::handle);

    tracing::info!("workers ready: consuming notification jobs");
    Monitor::new()
        .register(worker)
        .run_with_signal(tokio::signal::ctrl_c())
        .await?;

    tracing::info!("workers shut down");
    Ok(())
}
