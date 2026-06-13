use async_trait::async_trait;

use crate::{
    error::RepositoryError,
    ids::CommentId,
    model::{Comment, CommentEntity},
};

#[async_trait]
pub trait CommentRepository: Send + Sync {
    /// The comment scoped to its parent — a matching id under a different
    /// entity reads as `None`.
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

    async fn save(&self, comment: &Comment) -> Result<(), RepositoryError>;

    async fn delete(&self, entity: CommentEntity, id: CommentId) -> Result<(), RepositoryError>;
}
