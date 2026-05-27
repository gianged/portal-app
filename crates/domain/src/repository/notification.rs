use async_trait::async_trait;
use time::OffsetDateTime;

use crate::{
    error::RepositoryError,
    ids::{NotificationId, UserId},
    model::Notification,
};

#[async_trait]
pub trait NotificationRepository: Send + Sync {
    async fn find_by_id(&self, id: NotificationId)
    -> Result<Option<Notification>, RepositoryError>;

    async fn list_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: u32,
    ) -> Result<Vec<Notification>, RepositoryError>;

    async fn count_unread(&self, user_id: UserId) -> Result<u64, RepositoryError>;

    async fn save(&self, notification: &Notification) -> Result<(), RepositoryError>;

    async fn mark_read(
        &self,
        id: NotificationId,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError>;
}
