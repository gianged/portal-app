mod email;
mod fanout;

use std::sync::Arc;

use domain::{
    ids::{NotificationId, UserId},
    model::Notification,
    repository::NotificationRepository,
};
use time::OffsetDateTime;

use crate::{
    error::{Error, Result},
    permissions::Permissions,
};

pub use email::EmailNotifier;
pub use fanout::NotificationFanout;

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

    /// Marks one of the actor's notifications read.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or is not the recipient,
    /// `NotFound` if the notification does not exist, or a repository error if the
    /// datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, notification_id = ?notification_id))]
    pub async fn mark_read(&self, actor: UserId, notification_id: NotificationId) -> Result<()> {
        self.perms.require_active(actor).await?;
        let notification = self
            .notifications
            .find_by_id(notification_id)
            .await?
            .ok_or(Error::NotFound("notification"))?;
        if notification.recipient_user_id != actor {
            return Err(Error::Forbidden);
        }
        if notification.read_at.is_some() {
            return Ok(());
        }
        let now = OffsetDateTime::now_utc();
        self.notifications.mark_read(notification_id, now).await?;
        Ok(())
    }
}
