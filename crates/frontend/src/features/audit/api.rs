//! Admin audit-log data: the global most-recent feed and per-entity trails.

use uuid::Uuid;

use shared::dto::audit::AuditLogDto;

use crate::api::client;
use crate::api::error::FrontendError;

/// The most-recent audit entries across all entities (`GET /audit/feed`).
/// Admin-only; the server returns 403 for non-admins.
pub async fn feed(limit: u32) -> Result<Vec<AuditLogDto>, FrontendError> {
    let limit_s = limit.to_string();
    let q = client::query(&[("limit", &limit_s)]);
    client::get_json(&format!("/audit/feed{q}")).await
}

/// Per-entity audit history (`GET /audit`, admin-only); use the typed wrappers below
/// so the schema/table strings live in one place.
async fn for_entity(
    entity_schema: &str,
    entity_table: &str,
    entity_id: Uuid,
    limit: u32,
) -> Result<Vec<AuditLogDto>, FrontendError> {
    let id_s = entity_id.to_string();
    let limit_s = limit.to_string();
    let q = client::query(&[
        ("entity_schema", entity_schema),
        ("entity_table", entity_table),
        ("entity_id", &id_s),
        ("limit", &limit_s),
    ]);
    client::get_json(&format!("/audit{q}")).await
}

pub async fn request_trail(id: Uuid, limit: u32) -> Result<Vec<AuditLogDto>, FrontendError> {
    for_entity("project", "requests", id, limit).await
}

pub async fn ticket_trail(id: Uuid, limit: u32) -> Result<Vec<AuditLogDto>, FrontendError> {
    for_entity("ticket", "tickets", id, limit).await
}

pub async fn project_trail(id: Uuid, limit: u32) -> Result<Vec<AuditLogDto>, FrontendError> {
    for_entity("project", "projects", id, limit).await
}
