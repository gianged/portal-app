use async_trait::async_trait;

use crate::{
    error::RepositoryError,
    ids::{GroupId, UserId},
    model::{Group, Membership},
};

#[async_trait]
pub trait GroupRepository: Send + Sync {
    async fn find_group(&self, id: GroupId) -> Result<Option<Group>, RepositoryError>;

    /// Every group, ordered by name. Backs the org-wide group directory.
    async fn list_all(&self) -> Result<Vec<Group>, RepositoryError>;

    /// Returns the single group with `kind = It`, if present.
    async fn find_it_group(&self) -> Result<Option<Group>, RepositoryError>;

    async fn save_group(&self, group: &Group) -> Result<(), RepositoryError>;

    async fn find_membership(
        &self,
        group_id: GroupId,
        user_id: UserId,
    ) -> Result<Option<Membership>, RepositoryError>;

    async fn list_memberships_for_group(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<Membership>, RepositoryError>;

    async fn list_active_memberships_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<Membership>, RepositoryError>;

    /// Active memberships for a batch of users (deduplicated by the caller).
    /// Backs display-role resolution for denormalized user summaries.
    async fn list_active_memberships_for_users(
        &self,
        user_ids: &[UserId],
    ) -> Result<Vec<Membership>, RepositoryError>;

    async fn save_membership(&self, membership: &Membership) -> Result<(), RepositoryError>;
}
