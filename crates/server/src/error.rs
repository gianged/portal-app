use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use domain::error::StorageError;
use shared::dto::common::{ApiError, ErrorCode};

/// HTTP-facing error. Its `IntoResponse` impl is the single place where an
/// `application::Error` (or a handler-level validation failure) is mapped to a
/// status code and the stable `{ code, message }` body the frontend decodes.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] application::Error),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("validation failed: {0}")]
    Validation(String),
    /// Body rejected during extraction; keeps the rejection's own status
    /// (400 syntax, 413 too large, 415 media type, 422 data error).
    #[error("body rejected: {1}")]
    JsonRejection(StatusCode, String),
    /// Too many requests in the current window, surfaced by the rate-limit
    /// middleware as `429 Too Many Requests`.
    #[error("rate limit exceeded")]
    RateLimited,
}

/// Authentication / session failures, surfaced as `401 Unauthorized`. Distinct
/// from `application::Error::Forbidden` (403): 401 means we don't know who you
/// are, 403 means you may not do this.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("authentication required")]
    Missing,
    #[error("invalid or expired session")]
    Invalid,
    #[error("invalid email or password")]
    InvalidCredentials,
}

impl AppError {
    fn parts(&self) -> (StatusCode, ErrorCode, String) {
        match self {
            Self::Validation(message) => (
                StatusCode::BAD_REQUEST,
                ErrorCode::Validation,
                message.clone(),
            ),
            Self::JsonRejection(status, message) => {
                (*status, ErrorCode::Validation, message.clone())
            }
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                ErrorCode::RateLimited,
                "too many requests".to_owned(),
            ),
            Self::Auth(err) => match err {
                AuthError::Missing => (
                    StatusCode::UNAUTHORIZED,
                    ErrorCode::Unauthenticated,
                    "authentication required".to_owned(),
                ),
                AuthError::Invalid => (
                    StatusCode::UNAUTHORIZED,
                    ErrorCode::Unauthenticated,
                    "invalid or expired session".to_owned(),
                ),
                AuthError::InvalidCredentials => (
                    StatusCode::UNAUTHORIZED,
                    ErrorCode::InvalidCredentials,
                    "invalid email or password".to_owned(),
                ),
            },
            Self::Domain(err) => match err {
                application::Error::NotFound(what) => (
                    StatusCode::NOT_FOUND,
                    ErrorCode::NotFound,
                    format!("{what} not found"),
                ),
                application::Error::Validation(message) => (
                    StatusCode::BAD_REQUEST,
                    ErrorCode::Validation,
                    message.clone(),
                ),
                application::Error::Forbidden => (
                    StatusCode::FORBIDDEN,
                    ErrorCode::Forbidden,
                    "forbidden".to_owned(),
                ),
                application::Error::Conflict(message) => {
                    (StatusCode::CONFLICT, ErrorCode::Conflict, message.clone())
                }
                application::Error::Transition(err) => {
                    (StatusCode::CONFLICT, ErrorCode::Conflict, err.to_string())
                }
                // A crafted or malformed storage key is the caller's fault, not
                // a backend fault, and must not page anyone as a 500.
                application::Error::Storage(StorageError::InvalidKey(_)) => (
                    StatusCode::BAD_REQUEST,
                    ErrorCode::Validation,
                    "invalid storage key".to_owned(),
                ),
                application::Error::Repository(_)
                | application::Error::Storage(_)
                | application::Error::Event(_)
                | application::Error::Job(_)
                | application::Error::Render(_)
                | application::Error::Authz(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorCode::Internal,
                    "internal server error".to_owned(),
                ),
            },
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = self.parts();
        // Backend faults logged with detail; clients see a generic message, 4xx not logged.
        if status.is_server_error() {
            tracing::error!(error = %self, "request failed");
        }
        let body = ApiError { code, message };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::body;
    use domain::error::{EventError, JobError, RepositoryError, StorageError, TransitionError};

    /// Renders an `AppError` and decodes the wire body the frontend would see.
    async fn decode(err: AppError) -> (StatusCode, ApiError) {
        let response = err.into_response();
        let status = response.status();
        let bytes = body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("buffer response body");
        let body: ApiError = serde_json::from_slice(&bytes).expect("decode ApiError");
        (status, body)
    }

    #[tokio::test]
    async fn maps_every_variant_to_its_status_and_code() {
        let cases: Vec<(AppError, StatusCode, ErrorCode)> = vec![
            (
                AppError::Validation("bad field".to_owned()),
                StatusCode::BAD_REQUEST,
                ErrorCode::Validation,
            ),
            (
                AppError::JsonRejection(StatusCode::PAYLOAD_TOO_LARGE, "too big".to_owned()),
                StatusCode::PAYLOAD_TOO_LARGE,
                ErrorCode::Validation,
            ),
            (
                AppError::RateLimited,
                StatusCode::TOO_MANY_REQUESTS,
                ErrorCode::RateLimited,
            ),
            (
                AuthError::Missing.into(),
                StatusCode::UNAUTHORIZED,
                ErrorCode::Unauthenticated,
            ),
            (
                AuthError::Invalid.into(),
                StatusCode::UNAUTHORIZED,
                ErrorCode::Unauthenticated,
            ),
            (
                AuthError::InvalidCredentials.into(),
                StatusCode::UNAUTHORIZED,
                ErrorCode::InvalidCredentials,
            ),
            (
                application::Error::NotFound("user").into(),
                StatusCode::NOT_FOUND,
                ErrorCode::NotFound,
            ),
            (
                application::Error::Validation("nope".to_owned()).into(),
                StatusCode::BAD_REQUEST,
                ErrorCode::Validation,
            ),
            (
                application::Error::Forbidden.into(),
                StatusCode::FORBIDDEN,
                ErrorCode::Forbidden,
            ),
            (
                application::Error::Conflict("dup".to_owned()).into(),
                StatusCode::CONFLICT,
                ErrorCode::Conflict,
            ),
            (
                application::Error::Transition(TransitionError::invalid("open", "closed")).into(),
                StatusCode::CONFLICT,
                ErrorCode::Conflict,
            ),
            (
                application::Error::Repository(RepositoryError::Backend("db down".to_owned()))
                    .into(),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::Internal,
            ),
            (
                application::Error::Storage(StorageError::Backend("disk".to_owned())).into(),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::Internal,
            ),
            (
                application::Error::Event(EventError::Backend("bus".to_owned())).into(),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::Internal,
            ),
            (
                application::Error::Job(JobError::Backend("queue".to_owned())).into(),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::Internal,
            ),
            (
                application::Error::Authz("openfga down".to_owned()).into(),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::Internal,
            ),
            (
                application::Error::Storage(StorageError::InvalidKey("../x".to_owned())).into(),
                StatusCode::BAD_REQUEST,
                ErrorCode::Validation,
            ),
        ];

        for (err, want_status, want_code) in cases {
            let (status, body) = decode(err).await;
            assert_eq!(status, want_status, "status for code `{want_code:?}`");
            assert_eq!(body.code, want_code, "code mismatch");
            assert!(
                !body.message.is_empty(),
                "empty message for `{want_code:?}`"
            );
        }
    }

    #[tokio::test]
    async fn internal_errors_hide_backend_detail() {
        // The wire message must not leak the wrapped backend string.
        let (_, body) = decode(
            application::Error::Repository(RepositoryError::Backend("secret dsn".to_owned()))
                .into(),
        )
        .await;
        assert_eq!(body.message, "internal server error");
    }

    #[tokio::test]
    async fn client_facing_messages_are_preserved() {
        let (_, body) = decode(AppError::Validation("email is required".to_owned())).await;
        assert_eq!(body.message, "email is required");

        let (_, body) = decode(application::Error::Conflict("name taken".to_owned()).into()).await;
        assert_eq!(body.message, "name taken");
    }
}
