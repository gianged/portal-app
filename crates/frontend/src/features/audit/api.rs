//! Admin audit-log data: the global most-recent feed.

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
