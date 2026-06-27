use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::ids::{
    ChannelId, CommentId, MessageId, NotificationId, ProjectId, ProjectInviteId, RequestId,
    TicketId, UserId,
};
use super::{project::ProjectInviteStatus, request::RequestStatus, ticket::TicketStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: NotificationId,
    pub recipient_user_id: UserId,
    pub payload: NotificationPayload,
    pub read_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotificationPayload {
    Announcement {
        announcement_id: MessageId,
        channel_id: ChannelId,
    },
    Mention {
        message_id: MessageId,
        channel_id: ChannelId,
        mentioned_by: UserId,
    },
    TicketUrgent {
        ticket_id: TicketId,
    },
    RequestAssigned {
        request_id: RequestId,
    },
    RequestStatusChange {
        request_id: RequestId,
        from: RequestStatus,
        to: RequestStatus,
    },
    ProjectInvite {
        invite_id: ProjectInviteId,
        project_id: ProjectId,
    },
    TicketAssigned {
        ticket_id: TicketId,
    },
    TicketStatusChange {
        ticket_id: TicketId,
        from: TicketStatus,
        to: TicketStatus,
    },
    ProjectInviteResponse {
        invite_id: ProjectInviteId,
        project_id: ProjectId,
        status: ProjectInviteStatus,
    },
    TicketRaised {
        ticket_id: TicketId,
    },
    RequestComment {
        request_id: RequestId,
        comment_id: CommentId,
    },
    TicketComment {
        ticket_id: TicketId,
        comment_id: CommentId,
    },
    System {
        message: String,
    },
}

/// Filterable kind tag; repository queries take this without parsing the full payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    Announcement,
    Mention,
    TicketUrgent,
    RequestAssigned,
    RequestStatusChange,
    ProjectInvite,
    TicketAssigned,
    TicketStatusChange,
    ProjectInviteResponse,
    TicketRaised,
    RequestComment,
    TicketComment,
    System,
}

impl NotificationPayload {
    #[must_use]
    pub const fn kind(&self) -> NotificationKind {
        match self {
            Self::Announcement { .. } => NotificationKind::Announcement,
            Self::Mention { .. } => NotificationKind::Mention,
            Self::TicketUrgent { .. } => NotificationKind::TicketUrgent,
            Self::RequestAssigned { .. } => NotificationKind::RequestAssigned,
            Self::RequestStatusChange { .. } => NotificationKind::RequestStatusChange,
            Self::ProjectInvite { .. } => NotificationKind::ProjectInvite,
            Self::TicketAssigned { .. } => NotificationKind::TicketAssigned,
            Self::TicketStatusChange { .. } => NotificationKind::TicketStatusChange,
            Self::ProjectInviteResponse { .. } => NotificationKind::ProjectInviteResponse,
            Self::TicketRaised { .. } => NotificationKind::TicketRaised,
            Self::RequestComment { .. } => NotificationKind::RequestComment,
            Self::TicketComment { .. } => NotificationKind::TicketComment,
            Self::System { .. } => NotificationKind::System,
        }
    }
}

impl Notification {
    #[must_use]
    pub const fn kind(&self) -> NotificationKind {
        self.payload.kind()
    }

    #[must_use]
    pub const fn is_unread(&self) -> bool {
        self.read_at.is_none()
    }

    pub fn mark_read(&mut self, now: OffsetDateTime) {
        if self.read_at.is_none() {
            self.read_at = Some(now);
        }
    }
}
