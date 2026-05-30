use std::{collections::HashSet, sync::Arc};

use domain::{
    ids::{ChannelId, GroupId, NotificationId, UserId},
    model::{Channel, GroupRole, Membership, Notification, NotificationPayload, TicketPriority},
    repository::{
        ChatRepository, GroupRepository, NotificationRepository, RequestRepository, UserRepository,
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
/// It runs in the worker, off the request path, so it holds repositories
/// directly and performs no permission checks — it acts as the system, not a
/// user. That is also why it cannot reuse `NotificationService`, whose methods
/// all require an active actor and only read.
pub struct NotificationFanout {
    notifications: Arc<dyn NotificationRepository>,
    groups: Arc<dyn GroupRepository>,
    users: Arc<dyn UserRepository>,
    requests: Arc<dyn RequestRepository>,
    chats: Arc<dyn ChatRepository>,
}

impl NotificationFanout {
    #[must_use]
    pub fn new(
        notifications: Arc<dyn NotificationRepository>,
        groups: Arc<dyn GroupRepository>,
        users: Arc<dyn UserRepository>,
        requests: Arc<dyn RequestRepository>,
        chats: Arc<dyn ChatRepository>,
    ) -> Self {
        Self {
            notifications,
            groups,
            users,
            requests,
            chats,
        }
    }

    /// Dispatch one event. Variants that do not produce notifications are a
    /// no-op. The originating actor/sender is never notified of their own action.
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
                self.fan_out(recipients, *sender, &payload).await
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
                self.fan_out(mentions.iter().copied(), *sender, &payload)
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
                self.fan_out(recipients, *actor, &payload).await
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
                self.fan_out([*assignee], *actor, &payload).await
            }
            DomainEvent::RequestStatusChanged {
                request_id,
                from,
                to,
                actor,
                ..
            } => {
                // The event omits creator/assignee; fetch the request to learn
                // who cares about the status change.
                let mut recipients = Vec::new();
                if let Some(request) = self.requests.find_by_id(*request_id).await? {
                    recipients.push(request.creator_user_id);
                    if let Some(assignee) = request.assignee_user_id {
                        recipients.push(assignee);
                    }
                }
                let payload = NotificationPayload::RequestStatusChange {
                    request_id: *request_id,
                    from: *from,
                    to: *to,
                };
                self.fan_out(recipients, *actor, &payload).await
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
                self.fan_out(recipients, *actor, &payload).await
            }
            _ => Ok(()),
        }
    }

    /// Persist one notification per distinct recipient, skipping `exclude`.
    /// Every save is attempted even if an earlier one fails; the first error is
    /// returned afterwards so a single bad row does not drop the rest.
    async fn fan_out(
        &self,
        recipients: impl IntoIterator<Item = UserId>,
        exclude: UserId,
        payload: &NotificationPayload,
    ) -> Result<()> {
        let mut seen = HashSet::new();
        let targets: Vec<UserId> = recipients
            .into_iter()
            .filter(|r| *r != exclude && seen.insert(*r))
            .collect();
        if targets.is_empty() {
            return Ok(());
        }

        let now = OffsetDateTime::now_utc();
        let mut first_err = None;
        for recipient in targets {
            let notification = Notification {
                id: NotificationId(Uuid::now_v7()),
                recipient_user_id: recipient,
                payload: payload.clone(),
                read_at: None,
                created_at: now,
            };
            if let Err(e) = self.notifications.save(&notification).await
                && first_err.is_none()
            {
                first_err = Some(e);
            }
        }
        match first_err {
            Some(e) => Err(e.into()),
            None => Ok(()),
        }
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
            let page = self.users.list_active(ACTIVE_USER_PAGE, offset).await?;
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
