mod email;
mod fanout;

use std::sync::Arc;

use domain::{
    ids::{NotificationId, UserId},
    model::Notification,
    repository::NotificationRepository,
};
use time::OffsetDateTime;

use crate::{error::Result, permissions::Permissions};

pub use email::EmailNotifier;
pub use fanout::NotificationFanout;

/// Read side of per-user notifications: list, unread count, mark-read. Writes
/// come from the system-level [`NotificationFanout`].
pub struct NotificationService {
    notifications: Arc<dyn NotificationRepository>,
    perms: Arc<Permissions>,
}

impl NotificationService {
    #[must_use]
    pub fn new(notifications: Arc<dyn NotificationRepository>, perms: Arc<Permissions>) -> Self {
        Self {
            notifications,
            perms,
        }
    }

    /// Lists the actor's notifications, optionally only unread ones.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, or a repository error if
    /// the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, unread_only = ?unread_only, limit = ?limit))]
    pub async fn list_for_user(
        &self,
        actor: UserId,
        unread_only: bool,
        limit: u32,
    ) -> Result<Vec<Notification>> {
        self.perms.require_active(actor).await?;
        Ok(self
            .notifications
            .list_for_user(actor, unread_only, limit)
            .await?)
    }

    /// Counts the actor's unread notifications.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, or a repository error if
    /// the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn count_unread(&self, actor: UserId) -> Result<u64> {
        self.perms.require_active(actor).await?;
        Ok(self.notifications.count_unread(actor).await?)
    }

    /// Marks the given notifications read for the actor in one statement; an
    /// empty list marks all unread. Ids the actor does not own are ignored
    /// (ownership is enforced in the repository query). Returns the number
    /// marked.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, or a repository error if
    /// the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, ids = ids.len()))]
    pub async fn mark_read_many(&self, actor: UserId, ids: &[NotificationId]) -> Result<u64> {
        self.perms.require_active(actor).await?;
        let now = OffsetDateTime::now_utc();
        Ok(self.notifications.mark_read_many(actor, ids, now).await?)
    }
}
