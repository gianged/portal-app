use async_trait::async_trait;

use crate::{
    error::RepositoryError,
    ids::{GroupId, UserId},
    model::{Group, Membership},
    repository::OutboxRecord,
};

#[async_trait]
pub trait GroupRepository: Send + Sync {
    async fn find_group(&self, id: GroupId) -> Result<Option<Group>, RepositoryError>;

    /// Groups for a batch of ids; missing ids are simply absent. Backs batched
    /// summary resolution.
    async fn find_by_ids(&self, ids: &[GroupId]) -> Result<Vec<Group>, RepositoryError>;

    /// Every group, ordered by name. Backs the org-wide group directory.
    async fn list_all(&self) -> Result<Vec<Group>, RepositoryError>;

    /// Returns the single group with `kind = It`, if present.
    async fn find_it_group(&self) -> Result<Option<Group>, RepositoryError>;

    /// `outbox` rows commit in the same transaction as the entity write, so an
    /// audited event cannot be lost between commit and projection.
    async fn save_group(
        &self,
        group: &Group,
        outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError>;

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

    /// `outbox` rows commit in the same transaction as the entity write, so an
    /// audited event cannot be lost between commit and projection.
    async fn save_membership(
        &self,
        membership: &Membership,
        outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError>;

    /// Persists a batch of memberships in order, with `outbox` committed
    /// alongside. Default loops `save_membership` (outbox rides the first row);
    /// transactional backends override so the rows commit or fail as one unit
    /// (backs multi-row role changes like leadership transfer).
    async fn save_memberships(
        &self,
        memberships: &[Membership],
        outbox: &[OutboxRecord],
    ) -> Result<(), RepositoryError> {
        let mut outbox = Some(outbox);
        for membership in memberships {
            self.save_membership(membership, outbox.take().unwrap_or(&[]))
                .await?;
        }
        Ok(())
    }
}
