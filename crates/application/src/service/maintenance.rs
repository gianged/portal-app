use std::{collections::HashSet, sync::Arc};

use time::{Duration, OffsetDateTime};

use domain::{
    ports::file_storage::FileStorage,
    repository::{NotificationRepository, RequestRepository, UserRepository},
};

use crate::error::Result;

/// System-level maintenance routines invoked by the background workers. Holds no
/// `Permissions` — these run as the system, not on behalf of a user (mirroring
/// [`super::NotificationFanout`]).
pub struct MaintenanceService {
    notifications: Arc<dyn NotificationRepository>,
    requests: Arc<dyn RequestRepository>,
    users: Arc<dyn UserRepository>,
    storage: Arc<dyn FileStorage>,
}

impl MaintenanceService {
    #[must_use]
    pub fn new(
        notifications: Arc<dyn NotificationRepository>,
        requests: Arc<dyn RequestRepository>,
        users: Arc<dyn UserRepository>,
        storage: Arc<dyn FileStorage>,
    ) -> Self {
        Self {
            notifications,
            requests,
            users,
            storage,
        }
    }

    /// Deletes read notifications older than `retention` (relative to `now`),
    /// returning the number pruned. Unread notifications are never touched.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    pub async fn prune_read_notifications(
        &self,
        retention: Duration,
        now: OffsetDateTime,
    ) -> Result<u64> {
        let cutoff = now - retention;
        Ok(self.notifications.delete_read_before(cutoff).await?)
    }

    /// Sweeps stored upload objects that no attachment or avatar references,
    /// skipping anything modified within `grace` of `now` so an in-flight upload
    /// whose DB row has not committed yet is never deleted. Returns the count
    /// removed.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable, or a `Storage`
    /// error if listing or deleting stored objects fails.
    pub async fn sweep_orphan_uploads(&self, grace: Duration, now: OffsetDateTime) -> Result<u64> {
        let mut referenced: HashSet<String> = HashSet::new();
        referenced.extend(self.requests.list_all_attachment_keys().await?);
        referenced.extend(self.users.list_avatar_keys().await?);

        let cutoff = now - grace;
        let mut removed = 0_u64;
        for object in self.storage.list("").await? {
            // Too recent (possible in-flight upload) or still referenced: keep.
            if object.modified_at > cutoff || referenced.contains(&object.key) {
                continue;
            }
            self.storage.delete(&object.key).await?;
            removed += 1;
        }
        Ok(removed)
    }
}
