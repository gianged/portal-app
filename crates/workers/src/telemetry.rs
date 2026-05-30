use tracing_subscriber::{EnvFilter, fmt};

/// Initialises the global tracing subscriber. Log level is taken from `RUST_LOG`,
/// falling back to a sensible default.
pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,portal=debug"));
    fmt().with_env_filter(filter).init();
}
