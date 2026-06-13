//! Email side-channel for notifications: resolves recipients to addresses,
//! renders plain-text bodies, and enqueues onto the durable `emails` queue the
//! worker's SMTP consumer drains. Strictly best-effort — a failure here must
//! never fail (and thereby retry) the non-idempotent in-app fanout.

use std::sync::Arc;

use domain::{
    ids::UserId,
    model::{NotificationKind, NotificationPayload, UserStatus},
    ports::{job_queue::JobQueue, mailer::EmailMessage},
    repository::UserRepository,
};

pub struct EmailNotifier {
    users: Arc<dyn UserRepository>,
    queue: Arc<dyn JobQueue>,
    base_url: String,
}

impl EmailNotifier {
    #[must_use]
    pub fn new(users: Arc<dyn UserRepository>, queue: Arc<dyn JobQueue>, base_url: String) -> Self {
        Self {
            users,
            queue,
            base_url: base_url.trim_end_matches('/').to_owned(),
        }
    }

    /// Launch scope: assignments, mentions, and the ticket lifecycle get
    /// email; everything else stays in-app only. Exhaustive on purpose — a new
    /// kind forces an explicit decision here.
    const fn wants_email(kind: NotificationKind) -> bool {
        match kind {
            NotificationKind::Mention
            | NotificationKind::RequestAssigned
            | NotificationKind::TicketAssigned
            | NotificationKind::TicketRaised
            | NotificationKind::TicketUrgent
            | NotificationKind::TicketStatusChange => true,
            NotificationKind::Announcement
            | NotificationKind::RequestStatusChange
            | NotificationKind::ProjectInvite
            | NotificationKind::ProjectInviteResponse
            | NotificationKind::RequestComment
            | NotificationKind::TicketComment
            | NotificationKind::System => false,
        }
    }

    /// Plain-text subject + body with a deep link back to the portal. `None`
    /// for kinds outside the launch scope.
    fn render(&self, payload: &NotificationPayload) -> Option<(String, String)> {
        let (subject, line, path) = match payload {
            NotificationPayload::Mention { .. } => (
                "[Portal] You were mentioned",
                "Someone mentioned you in chat.",
                "/chat".to_owned(),
            ),
            NotificationPayload::RequestAssigned { request_id } => (
                "[Portal] Request assigned to you",
                "A work request was assigned to you.",
                format!("/requests/{}", request_id.0),
            ),
            NotificationPayload::TicketAssigned { ticket_id } => (
                "[Portal] Ticket assigned to you",
                "An IT ticket was assigned to you.",
                format!("/tickets/{}", ticket_id.0),
            ),
            NotificationPayload::TicketRaised { ticket_id } => (
                "[Portal] New ticket raised",
                "A new IT ticket was raised.",
                format!("/tickets/{}", ticket_id.0),
            ),
            NotificationPayload::TicketUrgent { ticket_id } => (
                "[Portal] Urgent ticket needs attention",
                "A ticket was triaged as urgent.",
                format!("/tickets/{}", ticket_id.0),
            ),
            NotificationPayload::TicketStatusChange { ticket_id, .. } => (
                "[Portal] Ticket status changed",
                "A ticket you are involved with changed status.",
                format!("/tickets/{}", ticket_id.0),
            ),
            _ => return None,
        };
        let body = format!("{line}\n\n{}{path}\n", self.base_url);
        Some((subject.to_owned(), body))
    }

    /// Enqueues one email per active recipient. Every failure (lookup, render,
    /// enqueue) is logged and swallowed.
    pub async fn notify(&self, recipients: &[UserId], payload: &NotificationPayload) {
        // TODO(roadmap): per-user email preference check goes here.
        if !Self::wants_email(payload.kind()) {
            return;
        }
        let Some((subject, body)) = self.render(payload) else {
            return;
        };
        for recipient in recipients {
            let user = match self.users.find_by_id(*recipient).await {
                Ok(Some(user)) => user,
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(error = %e, "email: recipient lookup failed");
                    continue;
                }
            };
            if user.status != UserStatus::Active {
                continue;
            }
            let message = EmailMessage {
                to: user.email,
                subject: subject.clone(),
                body: body.clone(),
            };
            let bytes = match serde_json::to_vec(&message) {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!(error = %e, "email: message serialization failed");
                    continue;
                }
            };
            if let Err(e) = self.queue.enqueue("emails", &bytes).await {
                tracing::warn!(error = %e, to = %message.to, "email: enqueue failed");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use domain::error::{JobError, RepositoryError};
    use domain::ids::TicketId;
    use domain::model::User;
    use std::sync::Mutex;
    use time::OffsetDateTime;
    use uuid::Uuid;

    struct FakeUsers {
        user: User,
    }

    #[async_trait]
    impl UserRepository for FakeUsers {
        async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError> {
            Ok((self.user.id == id).then(|| self.user.clone()))
        }
        async fn find_by_email(&self, _email: &str) -> Result<Option<User>, RepositoryError> {
            Ok(None)
        }
        async fn list_active(
            &self,
            _limit: u32,
            _offset: u32,
            _q: Option<&str>,
        ) -> Result<Vec<User>, RepositoryError> {
            Ok(Vec::new())
        }
        async fn save(&self, _user: &User) -> Result<(), RepositoryError> {
            Ok(())
        }
        async fn list_avatar_keys(&self) -> Result<Vec<String>, RepositoryError> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct RecordingQueue {
        sent: Mutex<Vec<Vec<u8>>>,
    }

    #[async_trait]
    impl JobQueue for RecordingQueue {
        async fn enqueue(&self, _queue: &str, payload: &[u8]) -> Result<(), JobError> {
            self.sent.lock().unwrap().push(payload.to_vec());
            Ok(())
        }
    }

    fn user(id: UserId, status: UserStatus) -> User {
        let now = OffsetDateTime::UNIX_EPOCH;
        User {
            id,
            email: "person@example.com".to_owned(),
            password_hash: String::new(),
            full_name: "Person".to_owned(),
            avatar_storage_key: None,
            phone: None,
            timezone: "UTC".to_owned(),
            status,
            system_role: None,
            first_logged_in_at: None,
            deactivated_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn notifier(status: UserStatus, queue: Arc<RecordingQueue>) -> (EmailNotifier, UserId) {
        let uid = UserId(Uuid::from_u128(1));
        let users = Arc::new(FakeUsers {
            user: user(uid, status),
        });
        (
            EmailNotifier::new(users, queue, "http://portal.test/".to_owned()),
            uid,
        )
    }

    #[tokio::test]
    async fn ticket_assignment_is_emailed_with_link() {
        let queue = Arc::new(RecordingQueue::default());
        let (notifier, uid) = notifier(UserStatus::Active, queue.clone());
        let payload = NotificationPayload::TicketAssigned {
            ticket_id: TicketId(Uuid::nil()),
        };
        notifier.notify(&[uid], &payload).await;
        let sent = queue.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let message: EmailMessage = serde_json::from_slice(&sent[0]).unwrap();
        assert_eq!(message.to, "person@example.com");
        assert!(message.body.contains("http://portal.test/tickets/"));
    }

    #[tokio::test]
    async fn announcements_are_not_emailed() {
        let queue = Arc::new(RecordingQueue::default());
        let (notifier, uid) = notifier(UserStatus::Active, queue.clone());
        let payload = NotificationPayload::Announcement {
            announcement_id: domain::ids::MessageId(Uuid::nil()),
            channel_id: domain::ids::ChannelId(Uuid::nil()),
        };
        notifier.notify(&[uid], &payload).await;
        assert!(queue.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn inactive_recipients_are_skipped() {
        let queue = Arc::new(RecordingQueue::default());
        let (notifier, uid) = notifier(UserStatus::Deactivated, queue.clone());
        let payload = NotificationPayload::TicketAssigned {
            ticket_id: TicketId(Uuid::nil()),
        };
        notifier.notify(&[uid], &payload).await;
        assert!(queue.sent.lock().unwrap().is_empty());
    }
}
