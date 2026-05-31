//! HTTP tracing layer (request span + latency). Thin wrapper over `tower-http`
//! so the desired stack reads uniformly in `app::build`.

use tower_http::{
    classify::{ServerErrorsAsFailures, SharedClassifier},
    trace::TraceLayer,
};

#[must_use]
pub fn layer() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
}
