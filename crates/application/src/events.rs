use std::sync::Arc;

use domain::{
    ids::{
        ChannelId, GroupId, MembershipId, MessageId, ProjectId, ProjectInviteId, RequestId,
        TicketId, UserId,
    },
    model::{
        Announcement, Group, GroupRole, Message, Project, ProjectInviteStatus, ProjectStatus,
        Request, RequestStatus, Ticket, TicketPriority, TicketStatus, User,
    },
    ports::{event_publisher::EventPublisher, job_queue::JobQueue},
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomainEvent {
    UserCreated {
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
        after: User,
    },
    UserActivated {
        user_id: UserId,
        at: OffsetDateTime,
        after: User,
    },
    UserDeactivated {
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
        before: User,
        after: User,
    },
    UserReactivated {
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
        after: User,
    },
    UserProfileUpdated {
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
        before: User,
        after: User,
    },

    GroupCreated {
        group_id: GroupId,
        actor: UserId,
        at: OffsetDateTime,
        after: Group,
    },
    GroupDeleted {
        group_id: GroupId,
        actor: UserId,
        at: OffsetDateTime,
        before: Group,
    },
    MembershipAdded {
        membership_id: MembershipId,
        group_id: GroupId,
        user_id: UserId,
        role: GroupRole,
        actor: UserId,
        at: OffsetDateTime,
    },
    MembershipRoleChanged {
        membership_id: MembershipId,
        group_id: GroupId,
        user_id: UserId,
        from: GroupRole,
        to: GroupRole,
        actor: UserId,
        at: OffsetDateTime,
    },
    MembershipDeactivated {
        membership_id: MembershipId,
        group_id: GroupId,
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
    },
    LeadershipTransferred {
        group_id: GroupId,
        from_user: UserId,
        to_user: UserId,
        actor: UserId,
        at: OffsetDateTime,
    },

    ProjectCreated {
        project_id: ProjectId,
        owner_group: GroupId,
        actor: UserId,
        at: OffsetDateTime,
        after: Project,
    },
    ProjectMetadataUpdated {
        project_id: ProjectId,
        actor: UserId,
        at: OffsetDateTime,
        before: Project,
        after: Project,
    },
    ProjectStatusChanged {
        project_id: ProjectId,
        from: ProjectStatus,
        to: ProjectStatus,
        actor: UserId,
        at: OffsetDateTime,
    },
    ProjectInviteSent {
        invite_id: ProjectInviteId,
        project_id: ProjectId,
        target_group: GroupId,
        actor: UserId,
        at: OffsetDateTime,
    },
    ProjectInviteResponded {
        invite_id: ProjectInviteId,
        project_id: ProjectId,
        target_group: GroupId,
        status: ProjectInviteStatus,
        actor: UserId,
        at: OffsetDateTime,
    },
    ProjectCollaboratorRemoved {
        project_id: ProjectId,
        group_id: GroupId,
        actor: UserId,
        at: OffsetDateTime,
    },

    RequestCreated {
        request_id: RequestId,
        project_id: ProjectId,
        actor: UserId,
        at: OffsetDateTime,
        after: Request,
    },
    RequestAssigned {
        request_id: RequestId,
        project_id: ProjectId,
        assignee: UserId,
        actor: UserId,
        at: OffsetDateTime,
    },
    RequestStatusChanged {
        request_id: RequestId,
        project_id: ProjectId,
        from: RequestStatus,
        to: RequestStatus,
        actor: UserId,
        at: OffsetDateTime,
    },

    TicketRaised {
        ticket_id: TicketId,
        requester: UserId,
        at: OffsetDateTime,
        after: Ticket,
    },
    TicketTriaged {
        ticket_id: TicketId,
        priority: TicketPriority,
        actor: UserId,
        at: OffsetDateTime,
    },
    TicketAssigned {
        ticket_id: TicketId,
        assignee: UserId,
        actor: UserId,
        at: OffsetDateTime,
    },
    TicketStatusChanged {
        ticket_id: TicketId,
        from: TicketStatus,
        to: TicketStatus,
        actor: UserId,
        at: OffsetDateTime,
    },

    MessagePosted {
        message_id: MessageId,
        channel_id: ChannelId,
        sender: UserId,
        mentions: Vec<UserId>,
        at: OffsetDateTime,
        after: Message,
    },
    MessageDeleted {
        message_id: MessageId,
        channel_id: ChannelId,
        actor: UserId,
        at: OffsetDateTime,
    },

    AnnouncementPosted {
        announcement_id: MessageId,
        channel_id: ChannelId,
        sender: UserId,
        at: OffsetDateTime,
        after: Announcement,
    },
    AnnouncementEdited {
        announcement_id: MessageId,
        channel_id: ChannelId,
        actor: UserId,
        at: OffsetDateTime,
        after: Announcement,
    },
    AnnouncementDeleted {
        announcement_id: MessageId,
        channel_id: ChannelId,
        actor: UserId,
        at: OffsetDateTime,
    },
}

impl DomainEvent {
    #[must_use]
    pub const fn topic(&self) -> &'static str {
        match self {
            Self::UserCreated { .. }
            | Self::UserActivated { .. }
            | Self::UserDeactivated { .. }
            | Self::UserReactivated { .. }
            | Self::UserProfileUpdated { .. } => "portal.user",
            Self::GroupCreated { .. }
            | Self::GroupDeleted { .. }
            | Self::MembershipAdded { .. }
            | Self::MembershipRoleChanged { .. }
            | Self::MembershipDeactivated { .. }
            | Self::LeadershipTransferred { .. } => "portal.group",
            Self::ProjectCreated { .. }
            | Self::ProjectMetadataUpdated { .. }
            | Self::ProjectStatusChanged { .. }
            | Self::ProjectInviteSent { .. }
            | Self::ProjectInviteResponded { .. }
            | Self::ProjectCollaboratorRemoved { .. } => "portal.project",
            Self::RequestCreated { .. }
            | Self::RequestAssigned { .. }
            | Self::RequestStatusChanged { .. } => "portal.request",
            Self::TicketRaised { .. }
            | Self::TicketTriaged { .. }
            | Self::TicketAssigned { .. }
            | Self::TicketStatusChanged { .. } => "portal.ticket",
            Self::MessagePosted { .. } | Self::MessageDeleted { .. } => "portal.chat",
            Self::AnnouncementPosted { .. }
            | Self::AnnouncementEdited { .. }
            | Self::AnnouncementDeleted { .. } => "portal.announcement",
        }
    }
}

/// Topics whose events can produce notifications. Only these are mirrored onto
/// the durable job queue for the worker to fan out; the rest (`portal.user`,
/// `portal.group`) are still broadcast for real-time consumers but never queued.
const NOTIFY_TOPICS: &[&str] = &[
    "portal.ticket",
    "portal.announcement",
    "portal.request",
    "portal.project",
    "portal.chat",
];

/// Dispatches every [`DomainEvent`] to two sinks: a broadcast publisher (Redis
/// pub/sub, for real-time WebSocket fan-out) and a durable job queue (apalis,
/// for background processing such as notifications). The same serialised bytes
/// feed both.
pub struct EventBus {
    publisher: Arc<dyn EventPublisher>,
    jobs: Arc<dyn JobQueue>,
}

impl EventBus {
    #[must_use]
    pub fn new(publisher: Arc<dyn EventPublisher>, jobs: Arc<dyn JobQueue>) -> Self {
        Self { publisher, jobs }
    }

    /// # Panics
    ///
    /// Panics only if `serde_json::to_vec` fails for `DomainEvent`, which would
    /// indicate a programming error — every variant is composed of types that
    /// implement `Serialize` infallibly.
    pub async fn emit(&self, event: DomainEvent) -> Result<()> {
        let topic = event.topic();
        let payload = serde_json::to_vec(&event)
            .expect("DomainEvent variants only contain serde-derivable types");
        self.publisher.publish(topic, &payload).await?;
        if NOTIFY_TOPICS.contains(&topic) {
            self.jobs.enqueue("notifications", &payload).await?;
        }
        Ok(())
    }
}
