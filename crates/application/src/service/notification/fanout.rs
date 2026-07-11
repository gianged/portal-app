use std::{collections::HashSet, sync::Arc};

use domain::{
    ids::{ChannelId, GroupId, NotificationId, ProjectInviteId, RequestId, TicketId, UserId},
    model::{
        Channel, CommentEntity, GroupRole, Membership, NotificationPayload, ProjectInviteStatus,
        TicketPriority, TicketStatus,
    },
    repository::{
        ChatRepository, GroupRepository, NotificationRepository, ProjectRepository,
        RequestRepository, TicketRepository, UserRepository,
    },
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{error::Result, events::DomainEvent};

/// Page size when fanning an announcement out to every active user (the general
/// channel). Sized for an org of 100-1000 users.
const ACTIVE_USER_PAGE: u32 = 500;

/// System-level handler that turns notification-producing [`DomainEvent`]s into
/// persisted [`Notification`] rows.
///
/// Runs in the worker off the request path, so it holds repositories directly and
/// performs no permission checks; it cannot reuse `NotificationService`, whose
/// methods all require an active actor and only read.
pub struct NotificationFanout {
    notifications: Arc<dyn NotificationRepository>,
    groups: Arc<dyn GroupRepository>,
    users: Arc<dyn UserRepository>,
    requests: Arc<dyn RequestRepository>,
    chats: Arc<dyn ChatRepository>,
    tickets: Arc<dyn TicketRepository>,
    projects: Arc<dyn ProjectRepository>,
    /// Optional email side-channel ([`Self::with_email`]); `None` keeps the
    /// fanout in-app only.
    email: Option<Arc<super::EmailNotifier>>,
}

impl NotificationFanout {
    #[must_use]
    pub fn new(
        notifications: Arc<dyn NotificationRepository>,
        groups: Arc<dyn GroupRepository>,
        users: Arc<dyn UserRepository>,
        requests: Arc<dyn RequestRepository>,
        chats: Arc<dyn ChatRepository>,
        tickets: Arc<dyn TicketRepository>,
        projects: Arc<dyn ProjectRepository>,
    ) -> Self {
        Self {
            notifications,
            groups,
            users,
            requests,
            chats,
            tickets,
            projects,
            email: None,
        }
    }

    /// Opt into emailing the subset of notification kinds the
    /// [`super::EmailNotifier`] covers, alongside the in-app rows.
    #[must_use]
    pub fn with_email(mut self, email: Arc<super::EmailNotifier>) -> Self {
        self.email = Some(email);
        self
    }

    /// Dispatch one event. Variants that do not produce notifications are a
    /// no-op. The originating actor/sender is never notified of their own action.
    ///
    /// # Errors
    /// Returns a repository error if resolving recipients (channels, groups,
    /// users, or the originating request) or persisting any notification fails.
    #[tracing::instrument(skip_all)]
    #[allow(clippy::too_many_lines)]
    pub async fn handle(&self, event: &DomainEvent) -> Result<()> {
        match event {
            DomainEvent::AnnouncementPosted {
                announcement_id,
                channel_id,
                sender,
                ..
            } => {
                let recipients = self.channel_recipient_ids(*channel_id).await?;
                let payload = NotificationPayload::Announcement {
                    announcement_id: *announcement_id,
                    channel_id: *channel_id,
                };
                self.fan_out(recipients, Some(*sender), &payload).await
            }
            DomainEvent::MessagePosted {
                message_id,
                channel_id,
                sender,
                mentions,
                ..
            } => {
                let payload = NotificationPayload::Mention {
                    message_id: *message_id,
                    channel_id: *channel_id,
                    mentioned_by: *sender,
                };
                self.fan_out(mentions.iter().copied(), Some(*sender), &payload)
                    .await
            }
            DomainEvent::TicketTriaged {
                ticket_id,
                priority,
                actor,
                ..
            } => {
                if *priority != TicketPriority::Urgent {
                    return Ok(());
                }
                let recipients = self.it_group_member_ids().await?;
                let payload = NotificationPayload::TicketUrgent {
                    ticket_id: *ticket_id,
                };
                self.fan_out(recipients, Some(*actor), &payload).await
            }
            DomainEvent::RequestAssigned {
                request_id,
                assignee,
                actor,
                ..
            } => {
                let payload = NotificationPayload::RequestAssigned {
                    request_id: *request_id,
                };
                self.fan_out([*assignee], Some(*actor), &payload).await
            }
            DomainEvent::RequestStatusChanged {
                request_id,
                from,
                to,
                actor,
                ..
            } => {
                let recipients = self.request_participants(*request_id).await?;
                let payload = NotificationPayload::RequestStatusChange {
                    request_id: *request_id,
                    from: *from,
                    to: *to,
                };
                self.fan_out(recipients, Some(*actor), &payload).await
            }
            DomainEvent::ProjectInviteSent {
                invite_id,
                project_id,
                target_group,
                actor,
                ..
            } => {
                let recipients = self.group_leader_id(*target_group).await?;
                let payload = NotificationPayload::ProjectInvite {
                    invite_id: *invite_id,
                    project_id: *project_id,
                };
                self.fan_out(recipients, Some(*actor), &payload).await
            }
            DomainEvent::TicketAssigned {
                ticket_id,
                assignee,
                actor,
                ..
            } => {
                let payload = NotificationPayload::TicketAssigned {
                    ticket_id: *ticket_id,
                };
                self.fan_out([*assignee], Some(*actor), &payload).await
            }
            DomainEvent::TicketStatusChanged {
                ticket_id,
                from,
                to,
                actor,
                ..
            } => {
                if !Self::is_notable_ticket_transition(*from, *to) {
                    return Ok(());
                }
                let recipients = self.ticket_participants(*ticket_id).await?;
                let payload = NotificationPayload::TicketStatusChange {
                    ticket_id: *ticket_id,
                    from: *from,
                    to: *to,
                };
                self.fan_out(recipients, Some(*actor), &payload).await
            }
            // Auto-close (null actor marks it system, not manual); reuses the status-change payload.
            DomainEvent::TicketAutoClosed { ticket_id, .. } => {
                let recipients = self.ticket_participants(*ticket_id).await?;
                let payload = NotificationPayload::TicketStatusChange {
                    ticket_id: *ticket_id,
                    from: TicketStatus::Resolved,
                    to: TicketStatus::Closed,
                };
                self.fan_out(recipients, None, &payload).await
            }
            DomainEvent::ProjectInviteResponded {
                invite_id,
                project_id,
                target_group,
                status,
                actor,
                ..
            } => {
                let recipients = self
                    .invite_response_recipients(*invite_id, *target_group, *status)
                    .await?;
                let payload = NotificationPayload::ProjectInviteResponse {
                    invite_id: *invite_id,
                    project_id: *project_id,
                    status: *status,
                };
                self.fan_out(recipients, Some(*actor), &payload).await
            }
            DomainEvent::TicketRaised {
                ticket_id,
                requester,
                ..
            } => {
                let recipients = self.it_group_member_ids().await?;
                let payload = NotificationPayload::TicketRaised {
                    ticket_id: *ticket_id,
                };
                self.fan_out(recipients, Some(*requester), &payload).await
            }
            // Notify work-item participants (creator/requester + assignee); `fan_out` excludes the commenter.
            DomainEvent::CommentAdded {
                comment_id,
                entity,
                actor,
                ..
            } => {
                let (recipients, payload) = match entity {
                    CommentEntity::Request { request_id } => (
                        self.request_participants(*request_id).await?,
                        NotificationPayload::RequestComment {
                            request_id: *request_id,
                            comment_id: *comment_id,
                        },
                    ),
                    CommentEntity::Ticket { ticket_id } => (
                        self.ticket_participants(*ticket_id).await?,
                        NotificationPayload::TicketComment {
                            ticket_id: *ticket_id,
                            comment_id: *comment_id,
                        },
                    ),
                };
                self.fan_out(recipients, Some(*actor), &payload).await
            }
            other => {
                tracing::debug!(
                    topic = other.topic(),
                    "fanout: event has no notification mapping"
                );
                Ok(())
            }
        }
    }

    /// Ticket transitions worth notifying the requester/assignee about: the
    /// resolution lifecycle (resolved/closed/reopened) and a rejected resolution
    /// (`Resolved -> InProgress`). `start` (`Assigned -> InProgress`) is noise.
    const fn is_notable_ticket_transition(from: TicketStatus, to: TicketStatus) -> bool {
        matches!(
            to,
            TicketStatus::Resolved | TicketStatus::Closed | TicketStatus::Reopened
        ) || matches!(
            (from, to),
            (TicketStatus::Resolved, TicketStatus::InProgress)
        )
    }

    /// Persist one notification per recipient, skipping `exclude` (the actor; `None` for system events).
    /// Recipients are written in batched statements so a company-wide fanout is a
    /// handful of round trips, not one per user.
    async fn fan_out(
        &self,
        recipients: impl IntoIterator<Item = UserId>,
        exclude: Option<UserId>,
        payload: &NotificationPayload,
    ) -> Result<()> {
        let mut seen = HashSet::new();
        let targets: Vec<UserId> = recipients
            .into_iter()
            .filter(|r| Some(*r) != exclude && seen.insert(*r))
            .collect();
        if targets.is_empty() {
            return Ok(());
        }

        let now = OffsetDateTime::now_utc();
        for chunk in targets.chunks(ACTIVE_USER_PAGE as usize) {
            let ids: Vec<NotificationId> = chunk
                .iter()
                .map(|_| NotificationId(Uuid::now_v7()))
                .collect();
            self.notifications
                .save_broadcast(&ids, chunk, payload, now)
                .await?;
        }
        // Email only after all saves succeed: a partial failure makes
        // apalis retry the job, and emailing first would double-send.
        if let Some(email) = &self.email {
            email.notify(&targets, payload).await;
        }
        Ok(())
    }

    /// Creator plus assignee of a request; events omit them, so the request is
    /// fetched. Empty when the request no longer exists.
    async fn request_participants(&self, request_id: RequestId) -> Result<Vec<UserId>> {
        let mut recipients = Vec::new();
        if let Some(request) = self.requests.find_by_id(request_id).await? {
            recipients.push(request.creator_user_id);
            if let Some(assignee) = request.assignee_user_id {
                recipients.push(assignee);
            }
        }
        Ok(recipients)
    }

    /// Requester plus assignee of a ticket (mirrors [`Self::request_participants`]).
    async fn ticket_participants(&self, ticket_id: TicketId) -> Result<Vec<UserId>> {
        let mut recipients = Vec::new();
        if let Some(ticket) = self.tickets.find_by_id(ticket_id).await? {
            recipients.push(ticket.requester_user_id);
            if let Some(assignee) = ticket.assignee_user_id {
                recipients.push(assignee);
            }
        }
        Ok(recipients)
    }

    /// Who hears about an invite response: the inviter on accept/decline, the
    /// invited group's leader on revoke.
    async fn invite_response_recipients(
        &self,
        invite_id: ProjectInviteId,
        target_group: GroupId,
        status: ProjectInviteStatus,
    ) -> Result<Vec<UserId>> {
        Ok(match status {
            // The event omits the inviter; fetch the invite for `invited_by_user_id`.
            ProjectInviteStatus::Accepted | ProjectInviteStatus::Declined => self
                .projects
                .find_invite(invite_id)
                .await?
                .map(|inv| inv.invited_by_user_id)
                .into_iter()
                .collect(),
            ProjectInviteStatus::Revoked => self
                .group_leader_id(target_group)
                .await?
                .into_iter()
                .collect(),
            // Not emitted for this event; defensively a no-op.
            ProjectInviteStatus::Pending => Vec::new(),
        })
    }

    /// Recipients of a channel-scoped notification (e.g. an announcement).
    async fn channel_recipient_ids(&self, channel_id: ChannelId) -> Result<Vec<UserId>> {
        let Some(channel) = self.chats.find_channel(channel_id).await? else {
            return Ok(Vec::new());
        };
        match channel {
            Channel::Group(c) => self.group_member_ids(c.group_id).await,
            Channel::General(_) => self.all_active_user_ids().await,
            // Direct channels carry no announcements (domain rule); nothing to do.
            Channel::Direct(_) => Ok(Vec::new()),
        }
    }

    async fn group_member_ids(&self, group_id: GroupId) -> Result<Vec<UserId>> {
        let memberships = self.groups.list_memberships_for_group(group_id).await?;
        Ok(memberships
            .into_iter()
            .filter(Membership::is_active)
            .map(|m| m.user_id)
            .collect())
    }

    async fn it_group_member_ids(&self) -> Result<Vec<UserId>> {
        match self.groups.find_it_group().await? {
            Some(group) => self.group_member_ids(group.id).await,
            None => Ok(Vec::new()),
        }
    }

    /// The active leader of a group, if any. `Option` is `IntoIterator`, so it
    /// feeds `fan_out` directly.
    async fn group_leader_id(&self, group_id: GroupId) -> Result<Option<UserId>> {
        let memberships = self.groups.list_memberships_for_group(group_id).await?;
        Ok(memberships
            .into_iter()
            .find(|m| m.is_active() && m.role == GroupRole::Leader)
            .map(|m| m.user_id))
    }

    async fn all_active_user_ids(&self) -> Result<Vec<UserId>> {
        let mut ids = Vec::new();
        let mut offset = 0;
        loop {
            let page = self
                .users
                .list_active(ACTIVE_USER_PAGE, offset, None)
                .await?;
            let page_len = page.len();
            ids.extend(page.into_iter().map(|u| u.id));
            if page_len < ACTIVE_USER_PAGE as usize {
                break;
            }
            offset += ACTIVE_USER_PAGE;
        }
        Ok(ids)
    }
}
