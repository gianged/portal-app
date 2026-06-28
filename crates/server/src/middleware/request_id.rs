//! Per-request correlation id: assigns a UUID v7, enters a tracing span, and
//! echoes it back to the client on the response as `x-request-id`.

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use tracing::{Instrument, field::Empty};
use uuid::Uuid;

pub async fn propagate(req: Request, next: Next) -> Response {
    let id = Uuid::now_v7();
    // `user_id` is declared empty here and filled by the auth layer once the
    // caller is known, so every log line under the request carries who + what.
    let span = tracing::info_span!(
        "http",
        request_id = %id,
        method = %req.method(),
        path = %req.uri().path(),
        user_id = Empty,
    );
    let mut res = next.run(req).instrument(span).await;
    // The hyphenated UUID form is always a valid header value.
    if let Ok(value) = HeaderValue::from_str(&id.to_string()) {
        res.headers_mut()
            .insert(HeaderName::from_static("x-request-id"), value);
    }
    res
}
