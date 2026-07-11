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
pub mod grpc;
pub mod middleware;
pub mod realtime;
pub mod resolve;
pub mod routes;

#[cfg(not(unix))]
use std::future;
use std::{net::SocketAddr, process, sync::LazyLock, time::Duration};

#[cfg(unix)]
use tokio::signal::unix::{self, SignalKind};
use tokio::{
    net::TcpListener,
    signal,
    sync::watch::{self, Sender},
    time,
};

use infrastructure::telemetry;

/// Grace after the shutdown signal before the watchdog forces exit. Must exceed
/// [`GRPC_DRAIN_GRACE`] plus the chat-ingest tail-flush budget so the watchdog
/// stays a last resort.
const FORCE_EXIT_GRACE: Duration = Duration::from_secs(15);

/// Bound on waiting for the internal gRPC plane to drain after HTTP stops.
const GRPC_DRAIN_GRACE: Duration = Duration::from_secs(5);

/// Process-wide drain flag, flipped to `true` on the first shutdown signal.
static SHUTDOWN: LazyLock<Sender<bool>> = LazyLock::new(|| watch::channel(false).0);

/// Resolves once process shutdown has begun. WS connection tasks select on it
/// so axum's graceful drain completes instead of stalling on live sockets.
pub(crate) async fn shutdown_started() {
    let mut rx = SHUTDOWN.subscribe();
    let _ = rx.wait_for(|&draining| draining).await;
}

/// Loads configuration, builds the router, and serves until a shutdown signal.
pub async fn run() -> anyhow::Result<()> {
    // Populate the process env from the repo-root .env before config is parsed.
    dotenvy::dotenv().ok();
    // Capture panics as structured logs, then stand up the log sinks. The guard
    // keeps the file-writer flush thread alive for the process lifetime.
    telemetry::install_panic_hook();
    let _log_guard = telemetry::init(&config::telemetry_config());

    let cfg = config::from_env()?;
    let (router, ingest, grpc) = app::build(&cfg).await?;

    let listener = TcpListener::bind(cfg.server_addr).await?;
    tracing::info!(addr = %cfg.server_addr, "server listening");
    // Last resort: guarantees Ctrl-C exits even if graceful shutdown stalls.
    tokio::spawn(force_exit_watchdog());
    // Internal gRPC plane on its own listener, sharing the shutdown signal. A
    // failure is logged, not fatal: the HTTP surface stays up without it.
    let mut grpc_task = tokio::spawn(async move {
        if let Err(error) = grpc.serve(shutdown_signal()).await {
            tracing::error!(%error, "internal grpc server failed");
        }
    });
    // `ConnectInfo` exposes the peer address to the per-IP rate limiter; graceful
    // shutdown lets in-flight requests and WebSocket connections drain on signal.
    let serve_result = axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await;
    // Drain the gRPC plane, then flush the chat ingest buffer's tail before
    // exit, even if serve returned an error, so optimistically-acked messages
    // still persist. The drain is bounded: after an abnormal serve error no
    // shutdown signal ever fires, and the gRPC task would wait forever.
    if time::timeout(GRPC_DRAIN_GRACE, &mut grpc_task)
        .await
        .is_err()
    {
        grpc_task.abort();
        tracing::warn!("internal grpc drain timed out; aborted");
    }
    ingest.shutdown().await;
    serve_result?;
    Ok(())
}

/// Force-exits [`FORCE_EXIT_GRACE`] after the signal if graceful shutdown hasn't
/// finished, so Ctrl-C reliably tears the process down.
async fn force_exit_watchdog() {
    wait_for_shutdown().await;
    time::sleep(FORCE_EXIT_GRACE).await;
    tracing::warn!("graceful shutdown timed out; forcing exit");
    process::exit(0);
}

/// Resolves on the first shutdown signal: Ctrl-C on every platform, plus
/// `SIGTERM` on Unix (the orchestrator's stop signal).
async fn shutdown_signal() {
    wait_for_shutdown().await;
    tracing::info!("shutdown signal received");
}

async fn wait_for_shutdown() {
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
    // Flip the drain flag so WS connections close and HTTP drain can finish.
    let _ = SHUTDOWN.send(true);
}
