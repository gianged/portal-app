//! Domain <-> wire projections for requests and their attachments.
//!
//! `request_status_dto` / `request_status_domain` are shared with notification
//! payloads (`super::notification`).

use application::commands::request::{CreateRequestCommand, UpdateRequestCommand};
use domain::{ids, model};
use shared::dto::{
    common::UserSummaryDto,
    request::{
        CreateRequestRequest, RequestAttachmentDto, RequestDto,
        RequestPriority as WireRequestPriority, RequestStatus as WireRequestStatus,
        UpdateRequestRequest,
    },
};

use super::{project_id, request_attachment_id, request_id};

#[must_use]
pub fn request_status_dto(status: model::RequestStatus) -> WireRequestStatus {
    match status {
        model::RequestStatus::Draft => WireRequestStatus::Draft,
        model::RequestStatus::Submitted => WireRequestStatus::Submitted,
        model::RequestStatus::Assigned => WireRequestStatus::Assigned,
        model::RequestStatus::InProgress => WireRequestStatus::InProgress,
        model::RequestStatus::Review => WireRequestStatus::Review,
        model::RequestStatus::Completed => WireRequestStatus::Completed,
        model::RequestStatus::Cancelled => WireRequestStatus::Cancelled,
    }
}

#[must_use]
pub fn request_status_domain(status: WireRequestStatus) -> model::RequestStatus {
    match status {
        WireRequestStatus::Draft => model::RequestStatus::Draft,
        WireRequestStatus::Submitted => model::RequestStatus::Submitted,
        WireRequestStatus::Assigned => model::RequestStatus::Assigned,
        WireRequestStatus::InProgress => model::RequestStatus::InProgress,
        WireRequestStatus::Review => model::RequestStatus::Review,
        WireRequestStatus::Completed => model::RequestStatus::Completed,
        WireRequestStatus::Cancelled => model::RequestStatus::Cancelled,
    }
}

#[must_use]
pub fn request_priority_dto(priority: model::RequestPriority) -> WireRequestPriority {
    match priority {
        model::RequestPriority::Low => WireRequestPriority::Low,
        model::RequestPriority::Normal => WireRequestPriority::Normal,
        model::RequestPriority::High => WireRequestPriority::High,
        model::RequestPriority::Urgent => WireRequestPriority::Urgent,
    }
}

#[must_use]
pub fn request_priority_domain(priority: WireRequestPriority) -> model::RequestPriority {
    match priority {
        WireRequestPriority::Low => model::RequestPriority::Low,
        WireRequestPriority::Normal => model::RequestPriority::Normal,
        WireRequestPriority::High => model::RequestPriority::High,
        WireRequestPriority::Urgent => model::RequestPriority::Urgent,
    }
}

#[must_use]
pub fn request_dto(
    request: &model::Request,
    creator: UserSummaryDto,
    assignee: Option<UserSummaryDto>,
) -> RequestDto {
    RequestDto {
        id: request_id(request.id),
        project_id: project_id(request.project_id),
        creator,
        assignee,
        title: request.title.clone(),
        description: request.description.clone(),
        status: request_status_dto(request.status),
        priority: request_priority_dto(request.priority),
        progress: request.progress,
        due_at: request.due_at,
        created_at: request.created_at,
        updated_at: request.updated_at,
    }
}

#[must_use]
pub fn request_attachment_dto(
    attachment: &model::RequestAttachment,
    uploaded_by: UserSummaryDto,
    download_url: String,
) -> RequestAttachmentDto {
    RequestAttachmentDto {
        id: request_attachment_id(attachment.id),
        filename: attachment.filename.clone(),
        content_type: attachment.content_type.clone(),
        size_bytes: attachment.size_bytes,
        download_url,
        uploaded_by,
        created_at: attachment.created_at,
    }
}

#[must_use]
pub fn create_request_command(req: CreateRequestRequest) -> CreateRequestCommand {
    CreateRequestCommand {
        project_id: ids::ProjectId(req.project_id.0),
        title: req.title,
        description: req.description,
        priority: request_priority_domain(req.priority),
        due_at: req.due_at,
    }
}

#[must_use]
pub fn update_request_command(req: UpdateRequestRequest) -> UpdateRequestCommand {
    UpdateRequestCommand {
        title: req.title,
        description: req.description,
        priority: req.priority.map(request_priority_domain),
        due_at: req.due_at,
    }
}
