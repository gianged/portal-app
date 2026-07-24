use async_trait::async_trait;

use crate::{error::RepositoryError, ids::UserId, model::User, repository::OutboxRecord};

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError>;

    /// Users for a batch of ids, any status (historical references must resolve);
    /// missing ids are simply absent. Backs batched summary resolution.
    async fn find_by_ids(&self, ids: &[UserId]) -> Result<Vec<User>, RepositoryError>;

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, RepositoryError>;

    /// `q` is a case-insensitive substring filter on name/email; `None` lists
    /// everyone.
    async fn list_active(
        &self,
        limit: u32,
        offset: u32,
        q: Option<&str>,
    ) -> Result<Vec<User>, RepositoryError>;

    /// `outbox` rows commit in the same transaction as the entity write, so an
    /// audited event cannot be lost between commit and projection.
    async fn save(&self, user: &User, outbox: &[OutboxRecord]) -> Result<(), RepositoryError>;

    /// Every non-null avatar storage key. Backs the upload orphan-sweep job.
    async fn list_avatar_keys(&self) -> Result<Vec<String>, RepositoryError>;

    /// Active users carrying a `system_role` (Director / HR). Backs report
    /// recipient enumeration for the scheduled mail.
    async fn list_with_system_role(&self) -> Result<Vec<User>, RepositoryError>;
}
