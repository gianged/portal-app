mod app;
mod config;
mod error;
mod telemetry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Populate the process env from the repo-root .env before config is parsed.
    dotenvy::dotenv().ok();
    telemetry::init();

    let cfg = config::from_env()?;
    let router = app::build(&cfg).await?;

    let listener = tokio::net::TcpListener::bind(cfg.server_addr).await?;
    tracing::info!(addr = %cfg.server_addr, "server listening");
    axum::serve(listener, router).await?;
    Ok(())
}
