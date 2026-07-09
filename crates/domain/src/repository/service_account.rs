use async_trait::async_trait;

use crate::{error::RepositoryError, ids::ServiceAccountId, model::ServiceAccount};

#[async_trait]
pub trait ServiceAccountRepository: Send + Sync {
    async fn create(&self, account: &ServiceAccount) -> Result<(), RepositoryError>;

    async fn find_by_id(
        &self,
        id: ServiceAccountId,
    ) -> Result<Option<ServiceAccount>, RepositoryError>;

    /// Active accounts only: a revoked key must never authenticate.
    async fn find_active_by_key_hash(
        &self,
        key_hash: &[u8],
    ) -> Result<Option<ServiceAccount>, RepositoryError>;

    /// Every account regardless of status, newest first.
    async fn list(&self) -> Result<Vec<ServiceAccount>, RepositoryError>;

    async fn save(&self, account: &ServiceAccount) -> Result<(), RepositoryError>;
}
