//! Authenticated file download, backing attachment + avatar `storage_key`s and
//! the URLs `FileStorage::presign_get` emits.
//!
//! Access control is "any authenticated active session + unguessable key". The
//! route is mounted under `/api/v1`, so `STORAGE_PUBLIC_BASE` must include that
//! prefix for presign URLs to resolve. Per-resource checks (map key -> resource
//! -> viewer) are a future refinement.

use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
    routing::get,
};

use domain::error::StorageError;
use domain::ports::file_storage::FileStorage;

use crate::{app::AppState, error::AppError, extractors::auth_user::AuthUser};

pub fn router() -> Router<AppState> {
    Router::new().route("/files/{*key}", get(download))
}

async fn download(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(key): Path<String>,
) -> Result<Response, AppError> {
    let bytes = state.storage.get(&key).await.map_err(|e| match e {
        StorageError::NotFound => AppError::Domain(application::Error::NotFound("file")),
        other @ StorageError::Backend(_) => AppError::Domain(application::Error::Storage(other)),
    })?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, guess_mime(&key))
        .header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, max-age=300"),
        )
        .body(Body::from(bytes))
        .map_err(|e| AppError::Validation(format!("failed to build file response: {e}")))
}

/// Storage drops the content type on write, so infer a sensible one from the
/// key's extension for inline rendering (images/PDFs); default to a download.
fn guess_mime(key: &str) -> HeaderValue {
    let ext = key
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "txt" => "text/plain; charset=utf-8",
        "json" => "application/json",
        _ => "application/octet-stream",
    };
    HeaderValue::from_static(mime)
}
