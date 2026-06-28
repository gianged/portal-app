use std::{collections::HashSet, sync::Arc};

use domain::{
    ports::file_storage::FileStorage,
    repository::{
        ChatAttachmentRepository, NotificationRepository, ReportArchiveRepository,
        RequestRepository, TicketRepository, UserRepository,
    },
};
use time::{Duration, OffsetDateTime};

use crate::{
    error::Result,
    events::{DomainEvent, EventBus},
};

/// System-level maintenance routines invoked by the background workers. Holds no
/// `Permissions`; they run as the system, not on behalf of a user.
pub struct MaintenanceService {
    notifications: Arc<dyn NotificationRepository>,
    requests: Arc<dyn RequestRepository>,
    tickets: Arc<dyn TicketRepository>,
    chat_attachments: Arc<dyn ChatAttachmentRepository>,
    users: Arc<dyn UserRepository>,
    reports: Arc<dyn ReportArchiveRepository>,
    storage: Arc<dyn FileStorage>,
    events: Arc<EventBus>,
}

impl MaintenanceService {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        notifications: Arc<dyn NotificationRepository>,
        requests: Arc<dyn RequestRepository>,
        tickets: Arc<dyn TicketRepository>,
        chat_attachments: Arc<dyn ChatAttachmentRepository>,
        users: Arc<dyn UserRepository>,
        reports: Arc<dyn ReportArchiveRepository>,
        storage: Arc<dyn FileStorage>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            notifications,
            requests,
            tickets,
            chat_attachments,
            users,
            reports,
            storage,
            events,
        }
    }

    /// Deletes read notifications older than `retention` (relative to `now`),
    /// returning the number pruned. Unread notifications are never touched.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all)]
    pub async fn prune_read_notifications(
        &self,
        retention: Duration,
        now: OffsetDateTime,
    ) -> Result<u64> {
        let cutoff = now - retention;
        Ok(self.notifications.delete_read_before(cutoff).await?)
    }

    /// Closes resolved tickets whose reopen window (`window`) has lapsed, using the
    /// normal domain transition so a raced manual close/reopen is skipped. Emits
    /// [`DomainEvent::TicketAutoClosed`] per ticket and returns the number closed.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable, or an event
    /// error if the event bus fails.
    #[tracing::instrument(skip_all)]
    pub async fn auto_close_resolved_tickets(
        &self,
        window: Duration,
        now: OffsetDateTime,
    ) -> Result<u64> {
        const BATCH: u32 = 100;
        let cutoff = now - window;
        let mut closed = 0_u64;
        loop {
            let batch = self.tickets.list_resolved_before(cutoff, BATCH).await?;
            let batch_len = batch.len();
            for mut ticket in batch {
                // A concurrent reopen/close between list and here makes the
                // transition invalid; skip that ticket, don't abort the sweep.
                if let Err(e) = ticket.close(now) {
                    tracing::debug!(ticket = %ticket.id.0, error = %e, "auto-close skipped");
                    continue;
                }
                self.tickets.save(&ticket).await?;
                self.events
                    .emit(DomainEvent::TicketAutoClosed {
                        ticket_id: ticket.id,
                        at: now,
                    })
                    .await?;
                closed += 1;
            }
            if batch_len < BATCH as usize {
                break;
            }
        }
        Ok(closed)
    }

    /// Sweeps stored upload objects that nothing references, skipping anything
    /// modified within `grace` of `now` (a possible in-flight upload whose DB row
    /// has not committed yet). Returns the count removed.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable, or a `Storage`
    /// error if listing or deleting stored objects fails.
    #[tracing::instrument(skip_all)]
    pub async fn sweep_orphan_uploads(&self, grace: Duration, now: OffsetDateTime) -> Result<u64> {
        let mut referenced: HashSet<String> = HashSet::new();
        referenced.extend(self.requests.list_all_attachment_keys().await?);
        referenced.extend(self.users.list_avatar_keys().await?);
        // Load-bearing: blocks deletion of in-use chat attachments.
        referenced.extend(self.chat_attachments.list_all_keys().await?);
        // Generated report PDFs live under STORAGE_ROOT too; keep them.
        referenced.extend(self.reports.list_all_storage_keys().await?);

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
