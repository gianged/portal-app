use async_trait::async_trait;

use crate::{
    error::RepositoryError,
    ids::CommentId,
    model::{Comment, CommentEntity},
    repository::OutboxRecord,
};

#[async_trait]
pub trait CommentRepository: Send + Sync {
    /// The comment scoped to its parent; a matching id under a different entity reads as `None`.
    async fn find_by_id(
        &self,
        entity: CommentEntity,
        id: CommentId,
    ) -> Result<Option<Comment>, RepositoryError>;

    /// Newest-first page; `before` is an exclusive cursor (same contract as
    /// `ChatRepository::list_messages`).
    async fn list_for_entity(
        &self,
        entity: CommentEntity,
        before: Option<CommentId>,
        limit: u32,
    ) -> Result<Vec<Comment>, RepositoryError>;

    /// `outbox` rows commit in the same transaction as the entity write, so an
    /// audited event cannot be lost between commit and projection.
    async fn save(&self, comment: &Comment, outbox: &[OutboxRecord])
    -> Result<(), RepositoryError>;

    /// `outbox` rows commit in the same transaction as the entity write, so an
    /// audited event cannot be lost between commit and projection.
    async fn delete(
        &self,
        entity: CommentEntity,
        id: CommentId,
        outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError>;
}
