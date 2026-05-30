mod bootstrap;
mod config;
mod telemetry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Populate the process env from the repo-root .env before config is parsed.
    dotenvy::dotenv().ok();
    telemetry::init();

    let cfg = config::from_env()?;
    bootstrap::connect(&cfg).await?;

    tracing::info!("workers ready (no jobs registered yet)");
    // Idle until shut down; the apalis Monitor replaces this when jobs land.
    std::future::pending::<()>().await;
    Ok(())
}
