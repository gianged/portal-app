use async_trait::async_trait;
use time::OffsetDateTime;

use crate::{
    error::RepositoryError,
    ids::{NotificationId, UserId},
    model::{Notification, NotificationPayload},
};

#[async_trait]
pub trait NotificationRepository: Send + Sync {
    async fn list_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: u32,
    ) -> Result<Vec<Notification>, RepositoryError>;

    async fn count_unread(&self, user_id: UserId) -> Result<u64, RepositoryError>;

    async fn save(&self, notification: &Notification) -> Result<(), RepositoryError>;

    /// Persists one unread notification per recipient with a shared payload in
    /// a single statement. `ids` supplies one pre-generated id per recipient
    /// and must match `recipients` in length. Backs company-wide fanout.
    async fn save_broadcast(
        &self,
        ids: &[NotificationId],
        recipients: &[UserId],
        payload: &NotificationPayload,
        created_at: OffsetDateTime,
    ) -> Result<(), RepositoryError>;

    /// Marks the given notifications read for `user_id` in one statement,
    /// returning the number updated. Ownership is enforced in the query; ids
    /// belonging to other users are ignored. An empty `ids` marks all unread.
    async fn mark_read_many(
        &self,
        user_id: UserId,
        ids: &[NotificationId],
        at: OffsetDateTime,
    ) -> Result<u64, RepositoryError>;

    /// Deletes read notifications whose `read_at` is older than `cutoff`,
    /// returning the number removed. Backs the maintenance retention sweep.
    async fn delete_read_before(&self, cutoff: OffsetDateTime) -> Result<u64, RepositoryError>;
}
