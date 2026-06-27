//! HTTP tracing layer (request span + latency); thin wrapper over `tower-http`.

use tower_http::{
    classify::{ServerErrorsAsFailures, SharedClassifier},
    trace::TraceLayer,
};

#[must_use]
pub fn layer() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
}
