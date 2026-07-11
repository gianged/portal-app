//! Domain <-> wire projection for notifications. Payload variants embed ids and
//! status enums projected by the sibling entity modules (re-exported via `super`).

use domain::model;
use shared::dto::notification::{NotificationDto, NotificationPayloadDto};

#[must_use]
pub fn notification_payload_dto(payload: &model::NotificationPayload) -> NotificationPayloadDto {
    match payload {
        model::NotificationPayload::Announcement {
            announcement_id,
            channel_id,
        } => NotificationPayloadDto::Announcement {
            announcement_id: super::message_id(*announcement_id),
            channel_id: super::channel_id(*channel_id),
        },
        model::NotificationPayload::Mention {
            message_id: msg,
            channel_id,
            mentioned_by,
        } => NotificationPayloadDto::Mention {
            message_id: super::message_id(*msg),
            channel_id: super::channel_id(*channel_id),
            mentioned_by: super::user_id(*mentioned_by),
        },
        model::NotificationPayload::TicketUrgent { ticket_id: tid } => {
            NotificationPayloadDto::TicketUrgent {
                ticket_id: super::ticket_id(*tid),
            }
        }
        model::NotificationPayload::RequestAssigned { request_id: rid } => {
            NotificationPayloadDto::RequestAssigned {
                request_id: super::request_id(*rid),
            }
        }
        model::NotificationPayload::RequestStatusChange {
            request_id: rid,
            from,
            to,
        } => NotificationPayloadDto::RequestStatusChange {
            request_id: super::request_id(*rid),
            from: super::request_status_dto(*from),
            to: super::request_status_dto(*to),
        },
        model::NotificationPayload::ProjectInvite {
            invite_id,
            project_id: pid,
        } => NotificationPayloadDto::ProjectInvite {
            invite_id: super::project_invite_id(*invite_id),
            project_id: super::project_id(*pid),
        },
        model::NotificationPayload::TicketAssigned { ticket_id: tid } => {
            NotificationPayloadDto::TicketAssigned {
                ticket_id: super::ticket_id(*tid),
            }
        }
        model::NotificationPayload::TicketStatusChange {
            ticket_id: tid,
            from,
            to,
        } => NotificationPayloadDto::TicketStatusChange {
            ticket_id: super::ticket_id(*tid),
            from: super::ticket_status_dto(*from),
            to: super::ticket_status_dto(*to),
        },
        model::NotificationPayload::ProjectInviteResponse {
            invite_id,
            project_id: pid,
            status,
        } => NotificationPayloadDto::ProjectInviteResponse {
            invite_id: super::project_invite_id(*invite_id),
            project_id: super::project_id(*pid),
            status: super::project_invite_status_dto(*status),
        },
        model::NotificationPayload::TicketRaised { ticket_id: tid } => {
            NotificationPayloadDto::TicketRaised {
                ticket_id: super::ticket_id(*tid),
            }
        }
        model::NotificationPayload::RequestComment {
            request_id: rid,
            comment_id: cid,
        } => NotificationPayloadDto::RequestComment {
            request_id: super::request_id(*rid),
            comment_id: super::comment_id(*cid),
        },
        model::NotificationPayload::TicketComment {
            ticket_id: tid,
            comment_id: cid,
        } => NotificationPayloadDto::TicketComment {
            ticket_id: super::ticket_id(*tid),
            comment_id: super::comment_id(*cid),
        },
        model::NotificationPayload::System { message } => NotificationPayloadDto::System {
            message: message.clone(),
        },
    }
}

#[must_use]
pub fn notification_dto(notification: &model::Notification) -> NotificationDto {
    NotificationDto {
        id: super::notification_id(notification.id),
        payload: notification_payload_dto(&notification.payload),
        read: notification.read_at.is_some(),
        created_at: notification.created_at,
    }
}
