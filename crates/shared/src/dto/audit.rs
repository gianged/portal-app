use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::dto::{common::UserSummaryDto, ids::AuditLogId};

/// Mirrors `domain::model::AuditAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    Create,
    Update,
    Delete,
    StatusChange,
    Assign,
    Transfer,
    Login,
    Logout,
}

impl AuditAction {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Create => "Create",
            Self::Update => "Update",
            Self::Delete => "Delete",
            Self::StatusChange => "Status Change",
            Self::Assign => "Assign",
            Self::Transfer => "Transfer",
            Self::Login => "Login",
            Self::Logout => "Logout",
        }
    }
}

/// Read-only audit-log row for an admin viewer; the `payload_before`/`after`
/// JSON blobs are omitted from this list shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogDto {
    pub id: AuditLogId,
    pub actor: Option<UserSummaryDto>,
    pub action: AuditAction,
    pub entity_schema: String,
    pub entity_table: String,
    /// Raw UUID; the referenced row lives in an arbitrary table, so no single
    /// newtype fits (mirrors `domain::model::AuditLog::entity_id`).
    pub entity_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
}
