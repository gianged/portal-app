use std::sync::Arc;

use domain::{
    ids::{
        ChannelId, CommentId, GroupId, MembershipId, MessageId, ProjectId, ProjectInviteId,
        RequestId, TicketId, UserId,
    },
    model::{
        Announcement, Comment, CommentEntity, Group, GroupRole, Message, Project,
        ProjectInviteStatus, ProjectStatus, Request, RequestStatus, Ticket, TicketPriority,
        TicketStatus, User,
    },
    ports::{event_publisher::EventPublisher, job_queue::JobQueue},
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::Result;

/// A business fact emitted by an application service after a successful state
/// change.
///
/// Serialized with an internally-tagged `snake_case` `type` discriminant and
/// routed by [`EventBus`] to a broadcast publisher (Redis pub/sub) and, for
/// notification-bearing topics, the durable job queue. [`Self::topic`] maps each
/// variant to its `portal.*` topic; the `before`/`after` payloads carry the
/// state that audit and notification consumers read.
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
    // No `User` payload — serializing it would put the password hash on the event bus.
    UserPasswordChanged {
        user_id: UserId,
        at: OffsetDateTime,
    },
    UserPasswordReset {
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
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
    GroupMetadataUpdated {
        group_id: GroupId,
        actor: UserId,
        at: OffsetDateTime,
        before: Group,
        after: Group,
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
    RequestMetadataUpdated {
        request_id: RequestId,
        project_id: ProjectId,
        actor: UserId,
        at: OffsetDateTime,
        before: Request,
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
    /// System close of a resolved ticket whose reopen window lapsed; no actor.
    TicketAutoClosed {
        ticket_id: TicketId,
        at: OffsetDateTime,
    },

    CommentAdded {
        comment_id: CommentId,
        entity: CommentEntity,
        actor: UserId,
        at: OffsetDateTime,
        after: Comment,
    },
    CommentEdited {
        comment_id: CommentId,
        entity: CommentEntity,
        actor: UserId,
        at: OffsetDateTime,
        after: Comment,
    },
    CommentDeleted {
        comment_id: CommentId,
        entity: CommentEntity,
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
    MessageEdited {
        message_id: MessageId,
        channel_id: ChannelId,
        actor: UserId,
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
            | Self::UserProfileUpdated { .. }
            | Self::UserPasswordChanged { .. }
            | Self::UserPasswordReset { .. } => "portal.user",
            Self::GroupCreated { .. }
            | Self::GroupDeleted { .. }
            | Self::GroupMetadataUpdated { .. }
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
            | Self::RequestMetadataUpdated { .. }
            | Self::RequestAssigned { .. }
            | Self::RequestStatusChanged { .. } => "portal.request",
            Self::TicketRaised { .. }
            | Self::TicketTriaged { .. }
            | Self::TicketAssigned { .. }
            | Self::TicketStatusChanged { .. }
            | Self::TicketAutoClosed { .. } => "portal.ticket",
            // Comments route by parent, reusing its already-notified/audited topic.
            Self::CommentAdded { entity, .. }
            | Self::CommentEdited { entity, .. }
            | Self::CommentDeleted { entity, .. } => match entity {
                CommentEntity::Request { .. } => "portal.request",
                CommentEntity::Ticket { .. } => "portal.ticket",
            },
            Self::MessagePosted { .. }
            | Self::MessageEdited { .. }
            | Self::MessageDeleted { .. } => "portal.chat",
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

/// Topics whose events are projected into the immutable audit log. Postgres-
/// backed entities only — `portal.chat` / `portal.announcement` live in Scylla
/// and are not audited here.
const AUDIT_TOPICS: &[&str] = &[
    "portal.user",
    "portal.group",
    "portal.project",
    "portal.request",
    "portal.ticket",
];

/// Dispatches every [`DomainEvent`] to two sinks: a broadcast publisher (Redis
/// pub/sub, for real-time WebSocket fan-out) and a durable job queue (apalis,
/// for background processing such as notifications). The same serialised bytes
/// feed both.
pub struct EventBus {
    publisher: Arc<dyn EventPublisher>,
    jobs: Arc<dyn JobQueue>,
    audit_jobs: Arc<dyn JobQueue>,
}

impl EventBus {
    #[must_use]
    pub fn new(
        publisher: Arc<dyn EventPublisher>,
        jobs: Arc<dyn JobQueue>,
        audit_jobs: Arc<dyn JobQueue>,
    ) -> Self {
        Self {
            publisher,
            jobs,
            audit_jobs,
        }
    }

    /// Publishes `event` to the broadcast publisher and, for notify topics, also
    /// enqueues it on the durable job queue.
    ///
    /// # Errors
    /// Returns an `Event` error if publishing to the broadcast publisher fails, or a `Job` error if enqueuing onto the job queue fails.
    ///
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
        if AUDIT_TOPICS.contains(&topic) {
            self.audit_jobs.enqueue("audit", &payload).await?;
        }
        Ok(())
    }

    /// Broadcasts `event` to the real-time publisher only, skipping the durable
    /// job queue. The batched chat drain uses this so it can enqueue
    /// notifications selectively rather than once per message.
    ///
    /// Unlike [`Self::emit`] this does NOT apply the `NOTIFY_TOPICS` /
    /// `AUDIT_TOPICS` routing: callers own that. Pairing it with a topic that
    /// belongs to `AUDIT_TOPICS` and not enqueuing the audit job elsewhere would
    /// silently drop an audit-log entry.
    ///
    /// # Errors
    /// Returns an `Event` error if publishing to the broadcast publisher fails.
    ///
    /// # Panics
    /// Panics only if `serde_json::to_vec` fails for `DomainEvent`, which cannot
    /// happen for these infallibly-serialisable variants.
    pub async fn broadcast(&self, event: &DomainEvent) -> Result<()> {
        let payload = serde_json::to_vec(event)
            .expect("DomainEvent variants only contain serde-derivable types");
        self.publisher.publish(event.topic(), &payload).await?;
        Ok(())
    }

    /// Enqueues `event` onto the durable notification queue only, skipping the
    /// broadcast. Pairs with [`Self::broadcast`] for callers that fan out a batch
    /// and enqueue notifications for just the events that need them. Like
    /// [`Self::broadcast`], it does not consult `NOTIFY_TOPICS` / `AUDIT_TOPICS`:
    /// the caller decides which events warrant a notification.
    ///
    /// # Errors
    /// Returns a `Job` error if enqueuing onto the job queue fails.
    ///
    /// # Panics
    /// Panics only if `serde_json::to_vec` fails for `DomainEvent`, which cannot
    /// happen for these infallibly-serialisable variants.
    pub async fn enqueue_notification(&self, event: &DomainEvent) -> Result<()> {
        let payload = serde_json::to_vec(event)
            .expect("DomainEvent variants only contain serde-derivable types");
        self.jobs.enqueue("notifications", &payload).await?;
        Ok(())
    }
}
