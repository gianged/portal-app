//! Domain <-> wire projection for notifications. Payload variants embed ids and
//! status enums projected by the sibling entity modules (re-exported via `super`).

use domain::model;
use shared::dto::notification::{NotificationDto, NotificationPayloadDto};

use super::{
    channel_id_wire, message_id, notification_id, project_id, project_invite_id,
    project_invite_status_dto, request_id, request_status_dto, ticket_id, ticket_status_dto,
    user_id,
};

#[must_use]
pub fn notification_payload_dto(payload: &model::NotificationPayload) -> NotificationPayloadDto {
    match payload {
        model::NotificationPayload::Announcement {
            announcement_id,
            channel_id,
        } => NotificationPayloadDto::Announcement {
            announcement_id: message_id(*announcement_id),
            channel_id: channel_id_wire(*channel_id),
        },
        model::NotificationPayload::Mention {
            message_id: msg,
            channel_id,
            mentioned_by,
        } => NotificationPayloadDto::Mention {
            message_id: message_id(*msg),
            channel_id: channel_id_wire(*channel_id),
            mentioned_by: user_id(*mentioned_by),
        },
        model::NotificationPayload::TicketUrgent { ticket_id: tid } => {
            NotificationPayloadDto::TicketUrgent {
                ticket_id: ticket_id(*tid),
            }
        }
        model::NotificationPayload::RequestAssigned { request_id: rid } => {
            NotificationPayloadDto::RequestAssigned {
                request_id: request_id(*rid),
            }
        }
        model::NotificationPayload::RequestStatusChange {
            request_id: rid,
            from,
            to,
        } => NotificationPayloadDto::RequestStatusChange {
            request_id: request_id(*rid),
            from: request_status_dto(*from),
            to: request_status_dto(*to),
        },
        model::NotificationPayload::ProjectInvite {
            invite_id,
            project_id: pid,
        } => NotificationPayloadDto::ProjectInvite {
            invite_id: project_invite_id(*invite_id),
            project_id: project_id(*pid),
        },
        model::NotificationPayload::TicketAssigned { ticket_id: tid } => {
            NotificationPayloadDto::TicketAssigned {
                ticket_id: ticket_id(*tid),
            }
        }
        model::NotificationPayload::TicketStatusChange {
            ticket_id: tid,
            from,
            to,
        } => NotificationPayloadDto::TicketStatusChange {
            ticket_id: ticket_id(*tid),
            from: ticket_status_dto(*from),
            to: ticket_status_dto(*to),
        },
        model::NotificationPayload::ProjectInviteResponse {
            invite_id,
            project_id: pid,
            status,
        } => NotificationPayloadDto::ProjectInviteResponse {
            invite_id: project_invite_id(*invite_id),
            project_id: project_id(*pid),
            status: project_invite_status_dto(*status),
        },
        model::NotificationPayload::TicketRaised { ticket_id: tid } => {
            NotificationPayloadDto::TicketRaised {
                ticket_id: ticket_id(*tid),
            }
        }
        model::NotificationPayload::System { message } => NotificationPayloadDto::System {
            message: message.clone(),
        },
    }
}

#[must_use]
pub fn notification_dto(notification: &model::Notification) -> NotificationDto {
    NotificationDto {
        id: notification_id(notification.id),
        payload: notification_payload_dto(&notification.payload),
        read: notification.read_at.is_some(),
        created_at: notification.created_at,
    }
}
