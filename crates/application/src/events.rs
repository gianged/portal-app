use std::sync::Arc;

use domain::{
    ids::{
        ChannelId, CommentId, DailyReportId, DayOffId, FlexHoursId, GroupId, LeaveGrantId,
        MembershipId, MessageId, OvertimeId, ProjectId, ProjectInviteId, RequestId, TicketId,
        UserId,
    },
    model::{
        Announcement, Comment, CommentEntity, FlexStatus, Group, GroupRole, Message,
        OvertimeStatus, Project, ProjectInviteStatus, ProjectStatus, Request, RequestStatus,
        Ticket, TicketPriority, TicketStatus,
    },
    ports::{
        event_publisher::EventPublisher,
        job_queue::{JobQueue, QUEUE_AUDIT, QUEUE_NOTIFICATIONS},
    },
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::Result;

/// A business fact emitted by an application service after a successful state change.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomainEvent {
    // No `User` payloads anywhere here: serializing one would put the password
    // hash on the event bus, and no consumer reads more than the ids.
    UserCreated {
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
    },
    UserActivated {
        user_id: UserId,
        at: OffsetDateTime,
    },
    UserDeactivated {
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
    },
    UserReactivated {
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
    },
    UserProfileUpdated {
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
    },
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
    RequestProgressUpdated {
        request_id: RequestId,
        project_id: ProjectId,
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

    AttendancePolicyUpdated {
        actor: UserId,
        at: OffsetDateTime,
    },

    DailyReportSubmitted {
        report_id: DailyReportId,
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
    },
    DailyReportReviewed {
        report_id: DailyReportId,
        user_id: UserId,
        approved: bool,
        actor: UserId,
        at: OffsetDateTime,
    },

    LeaveBalanceAdjusted {
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
    },
    /// System warning that a grant with a remainder is nearing expiry; no actor.
    LeaveBalanceExpiring {
        user_id: UserId,
        grant_id: LeaveGrantId,
        at: OffsetDateTime,
    },

    DayOffRequested {
        dayoff_id: DayOffId,
        user_id: UserId,
        actor: UserId,
        at: OffsetDateTime,
    },
    DayOffDecided {
        dayoff_id: DayOffId,
        user_id: UserId,
        approved: bool,
        actor: UserId,
        at: OffsetDateTime,
    },

    OvertimeRequested {
        overtime_id: OvertimeId,
        requester: UserId,
        at: OffsetDateTime,
    },
    OvertimeDecided {
        overtime_id: OvertimeId,
        requester: UserId,
        status: OvertimeStatus,
        actor: UserId,
        at: OffsetDateTime,
    },

    FlexRequested {
        flex_id: FlexHoursId,
        user_id: UserId,
        at: OffsetDateTime,
    },
    FlexDecided {
        flex_id: FlexHoursId,
        user_id: UserId,
        status: FlexStatus,
        actor: UserId,
        at: OffsetDateTime,
    },
    /// System warning that a user's approved flex hours do not net to the monthly
    /// expected total; no actor.
    FlexMonthUnreconciled {
        user_id: UserId,
        year: i32,
        month: u32,
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
            | Self::RequestStatusChanged { .. }
            | Self::RequestProgressUpdated { .. } => "portal.request",
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
            Self::AttendancePolicyUpdated { .. }
            | Self::DailyReportSubmitted { .. }
            | Self::DailyReportReviewed { .. }
            | Self::LeaveBalanceAdjusted { .. }
            | Self::LeaveBalanceExpiring { .. }
            | Self::DayOffRequested { .. }
            | Self::DayOffDecided { .. }
            | Self::OvertimeRequested { .. }
            | Self::OvertimeDecided { .. }
            | Self::FlexRequested { .. }
            | Self::FlexDecided { .. }
            | Self::FlexMonthUnreconciled { .. } => "portal.attendance",
        }
    }
}

/// Topics whose events are mirrored onto the durable job queue for the worker to
/// fan out as notifications.
const NOTIFY_TOPICS: &[&str] = &[
    "portal.ticket",
    "portal.announcement",
    "portal.request",
    "portal.project",
    "portal.chat",
];

/// Topics whose events are projected into the immutable audit log (Postgres-backed
/// entities only; `portal.chat` / `portal.announcement` live in Scylla).
const AUDIT_TOPICS: &[&str] = &[
    "portal.user",
    "portal.group",
    "portal.project",
    "portal.request",
    "portal.ticket",
];

/// Dispatches every [`DomainEvent`] to a broadcast publisher (Redis pub/sub) and a
/// durable job queue (apalis), feeding both the same serialised bytes.
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
    /// enqueues it on the durable job queue. With the dispatch chain absorbing
    /// transient failures, an enqueue error means the whole chain died: a lost
    /// notification is logged and dropped (best-effort), a lost audit entry
    /// fails the emit (the append-only log must not drop).
    ///
    /// # Errors
    /// Returns an `Event` error if publishing to the broadcast publisher fails, or a `Job` error if the audit enqueue fails.
    ///
    /// # Panics
    ///
    /// Panics only if `serde_json::to_vec` fails for `DomainEvent`, which cannot
    /// happen for these infallibly-serialisable variants.
    pub async fn emit(&self, event: DomainEvent) -> Result<()> {
        let topic = event.topic();
        let payload = serde_json::to_vec(&event)
            .expect("DomainEvent variants only contain serde-derivable types");
        self.publisher.publish(topic, &payload).await?;
        if NOTIFY_TOPICS.contains(&topic)
            && let Err(e) = self.jobs.enqueue(QUEUE_NOTIFICATIONS, &payload).await
        {
            tracing::error!(topic, error = %e, "notification enqueue failed; dropping fanout job");
        }
        if AUDIT_TOPICS.contains(&topic) {
            self.audit_jobs.enqueue(QUEUE_AUDIT, &payload).await?;
        }
        Ok(())
    }

    /// Broadcasts `event` to the real-time publisher only, skipping the durable job
    /// queue and the `NOTIFY_TOPICS` / `AUDIT_TOPICS` routing that callers own.
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
    /// broadcast and the `NOTIFY_TOPICS` / `AUDIT_TOPICS` routing. Best-effort
    /// like the notify path in [`Self::emit`]: a dead dispatch chain logs and
    /// drops instead of failing the caller.
    ///
    /// # Panics
    /// Panics only if `serde_json::to_vec` fails for `DomainEvent`, which cannot
    /// happen for these infallibly-serialisable variants.
    pub async fn enqueue_notification(&self, event: &DomainEvent) {
        let payload = serde_json::to_vec(event)
            .expect("DomainEvent variants only contain serde-derivable types");
        if let Err(e) = self.jobs.enqueue(QUEUE_NOTIFICATIONS, &payload).await {
            tracing::error!(error = %e, "notification enqueue failed; dropping fanout job");
        }
    }
}
