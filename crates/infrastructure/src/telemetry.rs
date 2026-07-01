//! Process-wide telemetry: structured log sinks, optional OpenTelemetry (OTLP)
//! span export, and a panic hook that turns an unwinding panic into a single
//! collectable log line instead of a silent thread death. Centralised here so
//! `server` and `workers` share one setup.
//!
//! Sinks behind the `RUST_LOG` `EnvFilter`:
//! - stdout in a [`LogFormat`] chosen for the run (tree for dev, json for prod);
//! - JSON to a daily-rolling file under `log_dir`;
//! - OTLP spans to a collector when `otlp_endpoint` is set (off by default).

use std::{
    backtrace::Backtrace, collections::HashMap, fs, panic, path::PathBuf, thread, time::Duration,
};

use opentelemetry::{
    global,
    trace::{TraceContextExt, TracerProvider as _},
};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::{Resource, propagation::TraceContextPropagator, trace::SdkTracerProvider};
use tracing::Span;
use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{
    EnvFilter, Layer, Registry, fmt, fmt::format::FmtSpan, layer::Layered, prelude::*,
};
use tracing_tree::HierarchicalLayer;

/// Per-request timeout for the OTLP span exporter.
const OTLP_TIMEOUT: Duration = Duration::from_secs(5);

/// A boxed layer over the `RUST_LOG`-filtered registry. The stdout, file, and
/// OTLP layers all erase to this shape so they can share one `Vec`.
type BoxedLayer = Box<dyn Layer<Layered<EnvFilter, Registry>> + Send + Sync>;

/// stdout format. The file sink is always JSON; this only shapes the console.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogFormat {
    /// Indented, nested spans with timing. Easiest to follow when debugging.
    #[default]
    Tree,
    /// Multi-line human format with span open/close events.
    Pretty,
    /// Single-line human format with span close events.
    Compact,
    /// One JSON object per line (machine-collectable stdout for prod).
    Json,
}

impl LogFormat {
    /// Parses the `LOG_FORMAT` env value; unknown or empty falls back to [`Tree`].
    #[must_use]
    pub fn from_env(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "pretty" => Self::Pretty,
            "compact" => Self::Compact,
            "json" => Self::Json,
            _ => Self::Tree,
        }
    }
}

/// Telemetry inputs assembled by the composition root (the only place env is read).
pub struct TelemetryConfig {
    pub log_dir: PathBuf,
    pub file_prefix: String,
    /// Service name attached to OTLP spans (`server` / `workers`).
    pub service_name: String,
    pub format: LogFormat,
    /// OTLP/HTTP collector base URL (e.g. `http://localhost:4318`). `None` disables export.
    pub otlp_endpoint: Option<String>,
}

/// Guard returned by [`init`]; hold it for the process lifetime. Flushes the file
/// writer and shuts the OTLP exporter down (draining buffered spans) on drop.
pub struct TelemetryGuard {
    _file_writer: WorkerGuard,
    provider: Option<SdkTracerProvider>,
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(provider) = &self.provider {
            let _ = provider.shutdown();
        }
    }
}

/// Installs a panic hook that logs each panic on target `panic` (thread, payload,
/// source location, captured backtrace) and returns without aborting. Combined
/// with the supervisor and the HTTP catch-panic layer, an unwinding panic in a
/// supervised task or a request handler is logged once and recovered.
pub fn install_panic_hook() {
    panic::set_hook(Box::new(|info| {
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
        let thread = thread::current();
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

/// Initialises the global subscriber from `cfg`: the [`LogFormat`] stdout layer,
/// the JSON file layer, and an optional OTLP layer, all behind the `RUST_LOG`
/// `EnvFilter`. Also installs the W3C trace-context propagator so a request's
/// trace id can ride along to background jobs. Returns the [`TelemetryGuard`] the
/// caller must hold for the process lifetime.
pub fn init(cfg: &TelemetryConfig) -> TelemetryGuard {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,portal=debug"));

    // Best-effort: a missing dir would otherwise just drop file logs silently.
    let _ = fs::create_dir_all(&cfg.log_dir);
    let file_appender = rolling::daily(&cfg.log_dir, &cfg.file_prefix);
    let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = fmt::layer()
        .json()
        .with_current_span(true)
        .with_writer(file_writer)
        .boxed();

    let stdout_layer: BoxedLayer = match cfg.format {
        LogFormat::Tree => HierarchicalLayer::new(2)
            .with_targets(true)
            .with_bracketed_fields(true)
            .with_ansi(true)
            .boxed(),
        LogFormat::Pretty => fmt::layer()
            .pretty()
            .with_target(true)
            .with_span_events(FmtSpan::CLOSE)
            .boxed(),
        LogFormat::Compact => fmt::layer()
            .compact()
            .with_target(true)
            .with_span_events(FmtSpan::CLOSE)
            .boxed(),
        LogFormat::Json => fmt::layer()
            .json()
            .with_current_span(true)
            .with_span_events(FmtSpan::CLOSE)
            .boxed(),
    };

    let mut layers: Vec<BoxedLayer> = vec![stdout_layer, file_layer];
    let provider = cfg.otlp_endpoint.as_deref().and_then(|endpoint| {
        let (layer, provider) = build_otlp_layer(endpoint, &cfg.service_name)?;
        layers.push(layer);
        Some(provider)
    });

    // Set the global propagator regardless of export: a no-op without an active
    // OTLP span, but lets `current_traceparent` work the moment export is on.
    global::set_text_map_propagator(TraceContextPropagator::new());

    tracing_subscriber::registry()
        .with(filter)
        .with(layers)
        .init();

    if let Some(endpoint) = &cfg.otlp_endpoint
        && provider.is_some()
    {
        tracing::info!(%endpoint, service = %cfg.service_name, "OTLP span export enabled");
    }

    TelemetryGuard {
        _file_writer: file_guard,
        provider,
    }
}

/// Builds the OTLP/HTTP span exporter, a batched tracer provider, and the
/// `tracing-opentelemetry` bridge layer. Returns `None` (logging unaffected) if
/// the exporter cannot be constructed.
fn build_otlp_layer(endpoint: &str, service_name: &str) -> Option<(BoxedLayer, SdkTracerProvider)> {
    let exporter = match SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .with_timeout(OTLP_TIMEOUT)
        .build()
    {
        Ok(exporter) => exporter,
        Err(error) => {
            // Telemetry not up yet, so stderr rather than tracing.
            eprintln!("OTLP exporter init failed ({error}); span export disabled");
            return None;
        }
    };
    let resource = Resource::builder()
        .with_service_name(service_name.to_owned())
        .build();
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();
    global::set_tracer_provider(provider.clone());
    let layer = tracing_opentelemetry::layer()
        .with_tracer(provider.tracer("portal"))
        .boxed();
    Some((layer, provider))
}

/// Captures the current span's trace context as a W3C `traceparent`, or `None`
/// when no OpenTelemetry span is active (OTLP export disabled). Call at the point
/// a job is enqueued so the worker can continue the same trace.
#[must_use]
pub fn current_traceparent() -> Option<String> {
    let cx = Span::current().context();
    if !cx.span().span_context().is_valid() {
        return None;
    }
    let mut carrier = HashMap::new();
    global::get_text_map_propagator(|p| p.inject_context(&cx, &mut carrier));
    carrier.remove("traceparent")
}

/// Links `span` to the trace identified by `traceparent`, making the enqueuing
/// request the parent of the job span. No-op on a blank value.
pub fn set_parent_traceparent(span: &Span, traceparent: &str) {
    if traceparent.is_empty() {
        return;
    }
    let mut carrier = HashMap::new();
    carrier.insert("traceparent".to_owned(), traceparent.to_owned());
    let cx = global::get_text_map_propagator(|p| p.extract(&carrier));
    let _ = span.set_parent(cx);
}
