use async_trait::async_trait;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{error::RepositoryError, model::AuditLog};

#[async_trait]
pub trait AuditRepository: Send + Sync {
    async fn append(&self, entry: &AuditLog) -> Result<(), RepositoryError>;

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
