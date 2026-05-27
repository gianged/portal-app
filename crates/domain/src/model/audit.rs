use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::ids::{AuditLogId, UserId};

/// Append-only record of state changes. The `payload_*` columns hold opaque
/// JSON snapshots — `serde_json` is not on the domain allow list, so they stay
/// `Option<String>` and infra owns the (de)serialisation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: AuditLogId,
    pub actor_user_id: Option<UserId>,
    pub action: AuditAction,
    pub entity_schema: String,
    pub entity_table: String,
    /// Raw `Uuid` — the row pointed at lives in an arbitrary table, so no
    /// single newtype fits.
    pub entity_id: Uuid,
    pub payload_before: Option<String>,
    pub payload_after: Option<String>,
    pub occurred_at: OffsetDateTime,
}

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
