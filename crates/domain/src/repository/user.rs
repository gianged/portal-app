use async_trait::async_trait;

use crate::{error::RepositoryError, ids::UserId, model::User};

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError>;

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, RepositoryError>;

    async fn list_active(&self, limit: u32, offset: u32) -> Result<Vec<User>, RepositoryError>;

    async fn save(&self, user: &User) -> Result<(), RepositoryError>;

    /// Every non-null avatar storage key. Backs the upload orphan-sweep job.
    async fn list_avatar_keys(&self) -> Result<Vec<String>, RepositoryError>;
}
