//! Process-wide telemetry: structured log sinks and a panic hook that turns an
//! unwinding panic into a single collectable log line instead of a silent thread
//! death. Centralised here so `server` and `workers` share one setup.

use std::{backtrace::Backtrace, path::Path};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Installs a panic hook that logs each panic on target `panic` (thread, payload,
/// source location, captured backtrace) and returns without aborting. Combined
/// with the supervisor and the HTTP catch-panic layer, an unwinding panic in a
/// supervised task or a request handler is logged once and recovered.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("<non-string panic payload>");
        let location = info.location().map_or_else(
            || "<unknown>".to_owned(),
            |l| format!("{}:{}:{}", l.file(), l.line(), l.column()),
        );
        let backtrace = Backtrace::force_capture();
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed>");
        tracing::error!(
            target: "panic",
            thread = thread_name,
            location = %location,
            backtrace = %backtrace,
            "panic: {message}"
        );
    }));
}

/// Initialises the global subscriber with two sinks behind the `RUST_LOG`
/// `EnvFilter`: pretty to stdout (dev) and JSON to a daily-rolling file under
/// `log_dir` (collect + debug later). Returns the [`WorkerGuard`] for the
/// non-blocking file writer; the caller must hold it for the process lifetime so
/// buffered log lines are flushed on exit.
pub fn init(log_dir: &Path, file_prefix: &str) -> WorkerGuard {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,portal=debug"));

    // Best-effort: a missing dir would otherwise just drop file logs silently.
    let _ = std::fs::create_dir_all(log_dir);
    let file_appender = tracing_appender::rolling::daily(log_dir, file_prefix);
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let stdout_layer = fmt::layer().with_target(true);
    let file_layer = fmt::layer()
        .json()
        .with_current_span(true)
        .with_writer(file_writer);

    tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();

    guard
}
