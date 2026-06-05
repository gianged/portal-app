use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use shared::dto::common::ApiError;

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
    /// Too many requests in the current window, surfaced by the rate-limit
    /// middleware as `429 Too Many Requests`.
    #[error("rate limit exceeded")]
    RateLimited,
}

/// Authentication / session failures, surfaced as `401 Unauthorized`. Kept
/// distinct from `application::Error::Forbidden` (403): a 401 means "we don't
/// know who you are" (no/!invalid session, bad credentials), a 403 means "we
/// know who you are, but you may not do this".
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
    fn parts(&self) -> (StatusCode, &'static str, String) {
        match self {
            Self::Validation(message) => (StatusCode::BAD_REQUEST, "validation", message.clone()),
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "too many requests".to_owned(),
            ),
            Self::Auth(err) => match err {
                AuthError::Missing => (
                    StatusCode::UNAUTHORIZED,
                    "unauthenticated",
                    "authentication required".to_owned(),
                ),
                AuthError::Invalid => (
                    StatusCode::UNAUTHORIZED,
                    "unauthenticated",
                    "invalid or expired session".to_owned(),
                ),
                AuthError::InvalidCredentials => (
                    StatusCode::UNAUTHORIZED,
                    "invalid_credentials",
                    "invalid email or password".to_owned(),
                ),
            },
            Self::Domain(err) => match err {
                application::Error::NotFound(what) => (
                    StatusCode::NOT_FOUND,
                    "not_found",
                    format!("{what} not found"),
                ),
                application::Error::Validation(message) => {
                    (StatusCode::BAD_REQUEST, "validation", message.clone())
                }
                application::Error::Forbidden => {
                    (StatusCode::FORBIDDEN, "forbidden", "forbidden".to_owned())
                }
                application::Error::Conflict(message) => {
                    (StatusCode::CONFLICT, "conflict", message.clone())
                }
                application::Error::Transition(err) => {
                    (StatusCode::CONFLICT, "conflict", err.to_string())
                }
                application::Error::Repository(_)
                | application::Error::Storage(_)
                | application::Error::Event(_)
                | application::Error::Job(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal server error".to_owned(),
                ),
            },
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = self.parts();
        // Backend faults are logged with detail; the client only sees a generic
        // message. Expected client errors (4xx) are not logged.
        if status.is_server_error() {
            tracing::error!(error = %self, "request failed");
        }
        let body = ApiError {
            code: code.to_owned(),
            message,
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use domain::error::{EventError, JobError, RepositoryError, StorageError, TransitionError};

    /// Renders an `AppError` and decodes the wire body the frontend would see.
    async fn decode(err: AppError) -> (StatusCode, ApiError) {
        let response = err.into_response();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("buffer response body");
        let body: ApiError = serde_json::from_slice(&bytes).expect("decode ApiError");
        (status, body)
    }

    #[tokio::test]
    async fn maps_every_variant_to_its_status_and_code() {
        let cases: Vec<(AppError, StatusCode, &str)> = vec![
            (
                AppError::Validation("bad field".to_owned()),
                StatusCode::BAD_REQUEST,
                "validation",
            ),
            (
                AppError::RateLimited,
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
            ),
            (
                AuthError::Missing.into(),
                StatusCode::UNAUTHORIZED,
                "unauthenticated",
            ),
            (
                AuthError::Invalid.into(),
                StatusCode::UNAUTHORIZED,
                "unauthenticated",
            ),
            (
                AuthError::InvalidCredentials.into(),
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
            ),
            (
                application::Error::NotFound("user").into(),
                StatusCode::NOT_FOUND,
                "not_found",
            ),
            (
                application::Error::Validation("nope".to_owned()).into(),
                StatusCode::BAD_REQUEST,
                "validation",
            ),
            (
                application::Error::Forbidden.into(),
                StatusCode::FORBIDDEN,
                "forbidden",
            ),
            (
                application::Error::Conflict("dup".to_owned()).into(),
                StatusCode::CONFLICT,
                "conflict",
            ),
            (
                application::Error::Transition(TransitionError::invalid("open", "closed")).into(),
                StatusCode::CONFLICT,
                "conflict",
            ),
            (
                application::Error::Repository(RepositoryError::Backend("db down".to_owned()))
                    .into(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
            ),
            (
                application::Error::Storage(StorageError::Backend("disk".to_owned())).into(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
            ),
            (
                application::Error::Event(EventError::Backend("bus".to_owned())).into(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
            ),
            (
                application::Error::Job(JobError::Backend("queue".to_owned())).into(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
            ),
        ];

        for (err, want_status, want_code) in cases {
            let (status, body) = decode(err).await;
            assert_eq!(status, want_status, "status for code `{want_code}`");
            assert_eq!(body.code, want_code, "code mismatch");
            assert!(!body.message.is_empty(), "empty message for `{want_code}`");
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
