use async_trait::async_trait;
use uuid::Uuid;

use crate::{audit::AuditLog, error::RepositoryError};

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
}
