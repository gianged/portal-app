//! Portal HTTP/WebSocket server.
//!
//! The library exposes the composition root ([`app::build`] / [`app::router`]),
//! config, auth, routes, and middleware so the `server` binary stays a thin
//! wrapper over [`run`] and the integration tests under `tests/` can drive the
//! real router against in-memory fakes.

#![allow(clippy::missing_errors_doc)]

pub mod app;
pub mod auth;
pub mod config;
pub mod dto;
pub mod error;
pub mod extractors;
pub mod middleware;
pub mod realtime;
pub mod resolve;
pub mod routes;

use std::{net::SocketAddr, path::Path, time::Duration};

/// Grace after the shutdown signal before the watchdog forces exit.
const FORCE_EXIT_GRACE: Duration = Duration::from_secs(3);

/// Loads configuration, builds the router, and serves until a shutdown signal.
pub async fn run() -> anyhow::Result<()> {
    // Populate the process env from the repo-root .env before config is parsed.
    dotenvy::dotenv().ok();
    // Capture panics as structured logs, then stand up the log sinks. The guard
    // keeps the file-writer flush thread alive for the process lifetime.
    infrastructure::telemetry::install_panic_hook();
    let _log_guard = infrastructure::telemetry::init(Path::new("logs"), "server");

    let cfg = config::from_env()?;
    let (router, ingest) = app::build(&cfg).await?;

    let listener = tokio::net::TcpListener::bind(cfg.server_addr).await?;
    tracing::info!(addr = %cfg.server_addr, "server listening");
    // Guarantees Ctrl-C exits even if graceful shutdown stalls (e.g. a live WS).
    tokio::spawn(force_exit_watchdog());
    // `ConnectInfo` exposes the peer address to the per-IP rate limiter; graceful
    // shutdown lets in-flight requests and WebSocket connections drain on signal.
    let serve_result = axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await;
    // HTTP server stopped: flush the chat ingest buffer's tail before exit, even
    // if serve returned an error, so optimistically-acked messages still persist.
    ingest.shutdown().await;
    serve_result?;
    Ok(())
}

/// Force-exits [`FORCE_EXIT_GRACE`] after the signal if graceful shutdown hasn't
/// finished, so Ctrl-C reliably tears the process down.
async fn force_exit_watchdog() {
    wait_for_shutdown().await;
    tokio::time::sleep(FORCE_EXIT_GRACE).await;
    tracing::warn!("graceful shutdown timed out; forcing exit");
    std::process::exit(0);
}

/// Resolves on the first shutdown signal: Ctrl-C on every platform, plus
/// `SIGTERM` on Unix (the orchestrator's stop signal).
async fn shutdown_signal() {
    wait_for_shutdown().await;
    tracing::info!("shutdown signal received");
}

async fn wait_for_shutdown() {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to install Ctrl-C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => tracing::error!(%error, "failed to install SIGTERM handler"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
