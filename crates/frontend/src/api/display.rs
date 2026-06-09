//! Presentation model for errors. Maps a [`FrontendError`] to the structured
//! Title / code / message shape the UI renders via
//! [`crate::primitives::error::ErrorCallout`] and the toast host. Keeps the
//! user-facing copy, severity, and request-id policy out of the wire contract.

use shared::dto::common::ErrorCode;

use crate::api::error::FrontendError;

/// Drives the callout's color and icon.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Danger,
    Warning,
    Info,
}

/// A ready-to-render error: a friendly title, an optional subtle code line, the
/// human message, and an optional support reference shown only on server faults.
#[derive(Clone)]
pub struct ErrorDisplay {
    pub title: String,
    pub code: Option<String>,
    pub message: String,
    pub request_id: Option<String>,
    pub severity: Severity,
}

impl From<&FrontendError> for ErrorDisplay {
    fn from(err: &FrontendError) -> Self {
        match err {
            FrontendError::Network(_) => Self {
                title: "Connection Problem".to_owned(),
                code: None,
                message: "We couldn't reach the server. Check your connection and try again."
                    .to_owned(),
                request_id: None,
                severity: Severity::Warning,
            },
            FrontendError::Serde(_) => Self {
                title: "Unexpected Response".to_owned(),
                code: None,
                message: "The server sent a response we couldn't read. Please try again."
                    .to_owned(),
                request_id: None,
                severity: Severity::Danger,
            },
            FrontendError::Validation(msg) => Self {
                title: "Check Your Input".to_owned(),
                code: None,
                message: msg.clone(),
                request_id: None,
                severity: Severity::Warning,
            },
            FrontendError::Http {
                status,
                code,
                message,
                request_id,
            } => {
                let code = *code;
                let status = *status;
                Self {
                    title: code.title().to_owned(),
                    code: Some(format!("HTTP {status} · {}", code.as_str())),
                    message: body_for(code, message),
                    // Only server faults are worth a reference the user reports.
                    request_id: if status >= 500 {
                        request_id.clone()
                    } else {
                        None
                    },
                    severity: severity_for(code),
                }
            }
        }
    }
}

/// Server faults send a deliberately generic wire message; replace it with
/// friendlier copy. Every other code carries useful detail, so keep it.
fn body_for(code: ErrorCode, server_message: &str) -> String {
    match code {
        ErrorCode::Internal | ErrorCode::Unknown => {
            "Something went wrong on our end. Please try again in a moment.".to_owned()
        }
        _ => server_message.to_owned(),
    }
}

fn severity_for(code: ErrorCode) -> Severity {
    match code {
        ErrorCode::Internal | ErrorCode::Unknown => Severity::Danger,
        ErrorCode::Validation
        | ErrorCode::Conflict
        | ErrorCode::RateLimited
        | ErrorCode::Forbidden => Severity::Warning,
        ErrorCode::NotFound | ErrorCode::Unauthenticated | ErrorCode::InvalidCredentials => {
            Severity::Info
        }
    }
}
