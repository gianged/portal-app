use async_trait::async_trait;

use crate::{error::RepositoryError, ids::UserId, model::AttendancePolicy};

/// Loads and persists the singleton attendance policy row.
#[async_trait]
pub trait PolicyRepository: Send + Sync {
    async fn load(&self) -> Result<AttendancePolicy, RepositoryError>;

    /// Overwrites the singleton, stamping `updated_by`.
    async fn save(
        &self,
        policy: &AttendancePolicy,
        updated_by: UserId,
    ) -> Result<(), RepositoryError>;
}
