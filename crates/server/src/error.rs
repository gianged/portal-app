use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use shared::dto::common::ApiError;

/// HTTP-facing error. Its `IntoResponse` impl is the single place where an
/// `application::Error` (or a handler-level validation failure) is mapped to a
/// status code and the stable `{ code, message }` body the frontend decodes.
///
/// Handlers are not implemented yet, so this is currently unused; it is the
/// contract they will return once the route modules land.
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] application::Error),
    #[error("validation failed: {0}")]
    Validation(String),
}

impl AppError {
    fn parts(&self) -> (StatusCode, &'static str, String) {
        match self {
            Self::Validation(message) => (StatusCode::BAD_REQUEST, "validation", message.clone()),
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
                | application::Error::Event(_) => (
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
