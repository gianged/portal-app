use async_trait::async_trait;

use crate::{error::RepositoryError, model::ChatAttachment};

/// Postgres chat-attachment metadata; separate from the Scylla `ChatRepository` (one adapter per backend).
#[async_trait]
pub trait ChatAttachmentRepository: Send + Sync {
    async fn save(&self, attachment: &ChatAttachment) -> Result<(), RepositoryError>;

    /// Resolve metadata for a set of storage keys (message rendering).
    async fn find_by_keys(&self, keys: &[String]) -> Result<Vec<ChatAttachment>, RepositoryError>;

    /// Every chat attachment's storage key. Backs the upload orphan-sweep job.
    async fn list_all_keys(&self) -> Result<Vec<String>, RepositoryError>;
}
