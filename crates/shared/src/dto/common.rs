use serde::{Deserialize, Serialize};

use crate::{
    dto::{
        group::GroupKind,
        ids::{GroupId, UserId},
        user::UserRole,
    },
    errors::SharedError,
};

/// Stable error body returned on every non-2xx response. The frontend
/// deserializes this instead of treating the body as opaque text. `code` is a
/// machine-stable discriminator (`"validation"`, `"forbidden"`, `"not_found"`,
/// `"conflict"`, `"internal"`); `message` is human-readable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl From<SharedError> for ApiError {
    fn from(err: SharedError) -> Self {
        match err {
            SharedError::Validation(message) => Self {
                code: "validation".to_owned(),
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

/// Cursor-paginated envelope. `next_cursor` is an opaque token the client
/// echoes back via [`PageQuery`] to fetch the following page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    /// Populated only by endpoints that can afford a `COUNT`.
    pub total: Option<u64>,
}

/// Pagination query parameters. The server clamps `limit` via
/// [`PageQuery::effective_limit`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

impl PageQuery {
    pub const DEFAULT_LIMIT: u32 = 20;
    pub const MAX_LIMIT: u32 = 100;

    /// The requested limit clamped to `1..=MAX_LIMIT`, defaulting to
    /// [`DEFAULT_LIMIT`](Self::DEFAULT_LIMIT) when unset.
    #[must_use]
    pub fn effective_limit(&self) -> u32 {
        self.limit
            .unwrap_or(Self::DEFAULT_LIMIT)
            .clamp(1, Self::MAX_LIMIT)
    }
}
