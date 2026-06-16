use async_trait::async_trait;

use crate::{error::RepositoryError, ids::UserId, model::User};

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError>;

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, RepositoryError>;

    /// `q` is a case-insensitive substring filter on name/email; `None` lists
    /// everyone.
    async fn list_active(
        &self,
        limit: u32,
        offset: u32,
        q: Option<&str>,
    ) -> Result<Vec<User>, RepositoryError>;

    async fn save(&self, user: &User) -> Result<(), RepositoryError>;

    /// Every non-null avatar storage key. Backs the upload orphan-sweep job.
    async fn list_avatar_keys(&self) -> Result<Vec<String>, RepositoryError>;

    /// Active users carrying a `system_role` (Director / HR). Backs report
    /// recipient enumeration for the scheduled mail.
    async fn list_with_system_role(&self) -> Result<Vec<User>, RepositoryError>;
}
