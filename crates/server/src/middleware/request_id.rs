//! Per-request correlation id. Assigns a UUID v7, enters a tracing span carrying
//! it (so all logs for one request correlate), and echoes it back to the client
//! on the response as `x-request-id`.

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use tracing::Instrument;
use uuid::Uuid;

pub async fn propagate(req: Request, next: Next) -> Response {
    let id = Uuid::now_v7();
    let span = tracing::info_span!("http", request_id = %id);
    let mut res = next.run(req).instrument(span).await;
    // The hyphenated UUID form is always a valid header value.
    if let Ok(value) = HeaderValue::from_str(&id.to_string()) {
        res.headers_mut()
            .insert(HeaderName::from_static("x-request-id"), value);
    }
    res
}
