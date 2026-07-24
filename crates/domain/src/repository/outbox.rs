use async_trait::async_trait;
use uuid::Uuid;

use crate::error::RepositoryError;

/// One audited domain event, captured in the same transaction as the entity
/// write it describes. The payload stays opaque bytes so the app-layer event
/// type never leaks into `domain`.
#[derive(Debug, Clone)]
pub struct OutboxRecord {
    pub id: Uuid,
    pub topic: String,
    pub payload: Vec<u8>,
}

/// Poll-side access to the audit outbox; the write side rides the audited
/// aggregates' repository methods as an `outbox` parameter.
#[async_trait]
pub trait OutboxRepository: Send + Sync {
    /// Oldest unprocessed records, up to `limit`. A plain read on purpose:
    /// projection is idempotent (audit rows dedup on the record id), so a
    /// double claim across restarts or instances converges.
    async fn claim_unprocessed(&self, limit: u32) -> Result<Vec<OutboxRecord>, RepositoryError>;

    /// Marks one record projected.
    async fn mark_processed(&self, id: Uuid) -> Result<(), RepositoryError>;
}
