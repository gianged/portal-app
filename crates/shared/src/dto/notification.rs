use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{
    ids::{
        ChannelId, MessageId, NotificationId, ProjectId, ProjectInviteId, RequestId, TicketId,
        UserId,
    },
    request::RequestStatus,
};

/// Filterable kind tag. Mirrors `domain::model::NotificationKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    Announcement,
    Mention,
    TicketUrgent,
    RequestAssigned,
    RequestStatusChange,
    ProjectInvite,
    System,
}

impl NotificationKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Announcement => "Announcement",
            Self::Mention => "Mention",
            Self::TicketUrgent => "Urgent Ticket",
            Self::RequestAssigned => "Request Assigned",
            Self::RequestStatusChange => "Request Update",
            Self::ProjectInvite => "Project Invite",
            Self::System => "System",
        }
    }
}

/// Tagged exactly like `domain::model::NotificationPayload` (same `kind` tag and
/// field names) so the wire format round-trips. IDs stay bare — the client
/// navigates by them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotificationPayloadDto {
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
    System {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationDto {
    pub id: NotificationId,
    pub payload: NotificationPayloadDto,
    /// Derived from `read_at`.
    pub read: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Mark notifications read. An empty list means "mark all".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarkReadRequest {
    pub notification_ids: Vec<NotificationId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;
    use uuid::Uuid;

    #[test]
    fn payload_tagged_by_kind() {
        let payload = NotificationPayloadDto::TicketUrgent {
            ticket_id: TicketId(Uuid::nil()),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"kind\":\"ticket_urgent\""), "got {json}");
        let back: NotificationPayloadDto = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, NotificationPayloadDto::TicketUrgent { .. }));
    }

    #[test]
    fn notification_uses_rfc3339() {
        let notification = NotificationDto {
            id: NotificationId(Uuid::nil()),
            payload: NotificationPayloadDto::System {
                message: "hi".to_owned(),
            },
            read: false,
            created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("valid ts"),
        };
        let json = serde_json::to_string(&notification).unwrap();
        assert!(json.contains("2023-11-14T22:13:20Z"), "got {json}");
        let back: NotificationDto = serde_json::from_str(&json).unwrap();
        assert_eq!(back.created_at, notification.created_at);
    }
}
