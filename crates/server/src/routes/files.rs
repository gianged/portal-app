//! Signed file download backing attachment + avatar `storage_key`s and the URLs
//! [`FileStorage::presign_get`] emits.
//!
//! Two-factor access: the signed `?exp&sig` query (HMAC over key + expiry +
//! viewer) plus a valid session for that viewer (mounted behind `require_auth`).
//! `STORAGE_PUBLIC_BASE` must share the app origin and include `/api/v1` so the
//! cookie rides along on `<img>`/`<a>` requests. Only raster images and PDFs
//! render inline; everything else (including script-bearing SVG) downloads as an
//! attachment with a generic content type.

use axum::{
    Router,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
    routing::get,
};
use serde::Deserialize;
use time::OffsetDateTime;

use domain::{error::StorageError, ports::file_storage::FileStorage};

use crate::{
    app::AppState,
    error::{AppError, AuthError},
    extractors::auth_user::AuthUser,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/files/{*key}", get(download))
}

/// Query string carried by a presigned download URL.
#[derive(Deserialize)]
struct SignedParams {
    /// Unix-seconds expiry embedded at sign time.
    exp: i64,
    /// Lowercase-hex HMAC-SHA256 over `key|exp|user`.
    sig: String,
}

async fn download(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(key): Path<String>,
    Query(params): Query<SignedParams>,
) -> Result<Response, AppError> {
    // Signature must be minted for the authenticated caller - a forwarded link fails here before expiry.
    if !state.signed_url.verify_for(
        &key,
        auth.user_id,
        params.exp,
        &params.sig,
        OffsetDateTime::now_utc(),
    ) {
        return Err(AppError::Auth(AuthError::Invalid));
    }

    let bytes = state.storage.get(&key).await.map_err(|e| match e {
        StorageError::NotFound => AppError::Domain(application::Error::NotFound("file")),
        other @ StorageError::Backend(_) => AppError::Domain(application::Error::Storage(other)),
    })?;

    let (mime, inline) = guess_mime(&key);
    let disposition = if inline {
        HeaderValue::from_static("inline")
    } else {
        attachment_disposition(&key)
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        )
        // Sandbox strips script execution + same-origin even if a doc type slips through; overrides the API-wide CSP.
        .header(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox"),
        )
        .header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, max-age=300"),
        )
        .body(Body::from(bytes))
        .map_err(|e| AppError::Validation(format!("failed to build file response: {e}")))
}

/// Infers a MIME from the key extension (storage drops it on write). Returns
/// `(mime, inline)`; only script-safe types render inline, SVG/unknown as downloads.
fn guess_mime(key: &str) -> (HeaderValue, bool) {
    let ext = key
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let (mime, inline) = match ext.as_str() {
        "png" => ("image/png", true),
        "jpg" | "jpeg" => ("image/jpeg", true),
        "gif" => ("image/gif", true),
        "webp" => ("image/webp", true),
        "pdf" => ("application/pdf", true),
        "txt" => ("text/plain; charset=utf-8", false),
        "json" => ("application/json", false),
        // SVG deliberately not image/svg+xml: inline SVG executes scripts.
        _ => ("application/octet-stream", false),
    };
    (HeaderValue::from_static(mime), inline)
}

/// `attachment; filename="…"` from the key's last segment, with characters
/// that could break the header quoting stripped.
fn attachment_disposition(key: &str) -> HeaderValue {
    let name: String = key
        .rsplit('/')
        .next()
        .unwrap_or("download")
        .chars()
        .filter(|c| !c.is_control() && *c != '"' && *c != '\\')
        .collect();
    let name = if name.is_empty() {
        "download".to_owned()
    } else {
        name
    };
    HeaderValue::from_str(&format!("attachment; filename=\"{name}\""))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}
