use serde::{Deserialize, Serialize};

use crate::{
    dto::{
        group::GroupKind,
        ids::{GroupId, UserId},
        user::UserRole,
    },
    errors::SharedError,
};

/// Machine-stable error discriminator carried in every [`ApiError`]; serializes
/// to `snake_case` wire tokens. Unknown codes deserialize to [`ErrorCode::Unknown`]
/// so an older frontend can still decode a newer backend's error body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Validation,
    RateLimited,
    Unauthenticated,
    InvalidCredentials,
    NotFound,
    Forbidden,
    Conflict,
    Internal,
    /// Catch-all for codes this build doesn't recognise; deserialize-only, never
    /// emitted by the backend.
    #[serde(other)]
    Unknown,
}

impl ErrorCode {
    /// Friendly heading the UI renders above the message.
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Validation => "Validation Error",
            Self::RateLimited => "Too Many Requests",
            Self::Unauthenticated => "Sign-in Required",
            Self::InvalidCredentials => "Sign-in Failed",
            Self::NotFound => "Not Found",
            Self::Forbidden => "Access Denied",
            Self::Conflict => "Conflict",
            Self::Internal => "Server Error",
            Self::Unknown => "Something Went Wrong",
        }
    }

    /// The `snake_case` wire token, for the "HTTP 500 · internal" detail line.
    /// Kept in sync with the serde rename by a unit test.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::RateLimited => "rate_limited",
            Self::Unauthenticated => "unauthenticated",
            Self::InvalidCredentials => "invalid_credentials",
            Self::NotFound => "not_found",
            Self::Forbidden => "forbidden",
            Self::Conflict => "conflict",
            Self::Internal => "internal",
            Self::Unknown => "unknown",
        }
    }
}

/// Stable error body returned on every non-2xx response; [`ErrorCode`] is a
/// machine-stable discriminator the frontend maps to a friendly title, and
/// `message` is human-readable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

impl From<SharedError> for ApiError {
    fn from(err: SharedError) -> Self {
        match err {
            SharedError::Validation(message) => Self {
                code: ErrorCode::Validation,
                message,
            },
        }
    }
}

/// Denormalized user reference embedded wherever a name/avatar is shown
/// (creator, assignee, sender, …) so the UI renders without a second fetch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSummaryDto {
    pub id: UserId,
    pub full_name: String,
    pub avatar_storage_key: Option<String>,
    pub role: UserRole,
}

/// Denormalized group reference embedded in project / collaborator views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSummaryDto {
    pub id: GroupId,
    pub name: String,
    pub kind: GroupKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::{Value, json};

    const ALL: [ErrorCode; 9] = [
        ErrorCode::Validation,
        ErrorCode::RateLimited,
        ErrorCode::Unauthenticated,
        ErrorCode::InvalidCredentials,
        ErrorCode::NotFound,
        ErrorCode::Forbidden,
        ErrorCode::Conflict,
        ErrorCode::Internal,
        ErrorCode::Unknown,
    ];

    #[test]
    fn error_code_serializes_to_its_wire_token() {
        for code in ALL {
            let json = serde_json::to_value(code).expect("serialize");
            assert_eq!(json, Value::String(code.as_str().to_owned()));
        }
    }

    #[test]
    fn unknown_codes_deserialize_to_unknown() {
        let parsed: ErrorCode = serde_json::from_value(json!("teapot")).expect("deserialize");
        assert_eq!(parsed, ErrorCode::Unknown);
    }

    #[test]
    fn api_error_round_trips() {
        let original = ApiError {
            code: ErrorCode::Internal,
            message: "internal server error".to_owned(),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        assert_eq!(
            json,
            r#"{"code":"internal","message":"internal server error"}"#
        );
        let back: ApiError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.code, original.code);
        assert_eq!(back.message, original.message);
    }
}
