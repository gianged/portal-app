use async_trait::async_trait;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{error::RepositoryError, model::AuditLog};

#[async_trait]
pub trait AuditRepository: Send + Sync {
    /// Appends one entry keyed by the outbox `event_id` that produced it; a
    /// redelivered projection deduplicates instead of double-appending.
    async fn append_dedup(&self, entry: &AuditLog, event_id: Uuid) -> Result<(), RepositoryError>;

    async fn list_for_entity(
        &self,
        entity_schema: &str,
        entity_table: &str,
        entity_id: Uuid,
        limit: u32,
    ) -> Result<Vec<AuditLog>, RepositoryError>;

    /// Most-recent audit rows across all entities, for the admin feed. `before`
    /// pages backwards by `occurred_at` (exclusive); `None` starts at the newest.
    async fn list_recent(
        &self,
        limit: u32,
        before: Option<OffsetDateTime>,
    ) -> Result<Vec<AuditLog>, RepositoryError>;
}
