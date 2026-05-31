mod app;
mod auth;
mod config;
mod dto;
mod error;
mod extractors;
mod middleware;
mod realtime;
mod resolve;
mod routes;
mod telemetry;

/// Builds the runtime on a thread with a generous stack. Composing the full
/// router (many nested route groups + middleware) produces deep stack frames in
/// debug builds that overflow Windows' default 1 MiB main-thread stack; an 8 MiB
/// stack gives the composition root ample headroom.
fn main() -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("server-main".to_owned())
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(run())
        })?
        .join()
        .expect("server thread panicked")
}

async fn run() -> anyhow::Result<()> {
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
