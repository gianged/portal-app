use shared::dto::common::ErrorCode;
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum FrontendError {
    #[error("network error: {0}")]
    Network(String),
    #[error("server returned HTTP {status}: {message}")]
    Http {
        status: u16,
        code: ErrorCode,
        message: String,
        /// `x-request-id` echoed by the server; surfaced on 5xx for support.
        request_id: Option<String>,
    },
    #[error("invalid response: {0}")]
    Serde(String),
    #[allow(dead_code)] // TODO: unused
    #[error("{0}")]
    Validation(String),
}

impl From<gloo::net::Error> for FrontendError {
    fn from(err: gloo::net::Error) -> Self {
        match err {
            gloo::net::Error::JsError(e) => Self::Network(e.to_string()),
            gloo::net::Error::SerdeError(e) => Self::Serde(e.to_string()),
            gloo::net::Error::GlooError(msg) => Self::Network(msg),
        }
    }
}
