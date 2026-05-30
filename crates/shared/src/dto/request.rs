use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{
    common::UserSummaryDto,
    ids::{ProjectId, RequestAttachmentId, RequestId, UserId},
};

/// Mirrors `domain::model::RequestStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Draft,
    Submitted,
    Assigned,
    InProgress,
    Review,
    Completed,
    Cancelled,
}

impl RequestStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Submitted => "Submitted",
            Self::Assigned => "Assigned",
            Self::InProgress => "In Progress",
            Self::Review => "Review",
            Self::Completed => "Completed",
            Self::Cancelled => "Cancelled",
        }
    }
}

/// Mirrors `domain::model::RequestPriority`. Request-specific, so it lives here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl RequestPriority {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Normal => "Normal",
            Self::High => "High",
            Self::Urgent => "Urgent",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestAttachmentDto {
    pub id: RequestAttachmentId,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub uploaded_by: UserSummaryDto,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestDto {
    pub id: RequestId,
    pub project_id: ProjectId,
    pub creator: UserSummaryDto,
    pub assignee: Option<UserSummaryDto>,
    pub title: String,
    pub description: String,
    pub status: RequestStatus,
    pub priority: RequestPriority,
    #[serde(with = "time::serde::rfc3339::option")]
    pub due_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestDetailDto {
    pub request: RequestDto,
    pub attachments: Vec<RequestAttachmentDto>,
}

/// Maps to `application::commands::CreateRequestCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRequestRequest {
    pub project_id: ProjectId,
    pub title: String,
    pub description: String,
    pub priority: RequestPriority,
    #[serde(with = "time::serde::rfc3339::option")]
    pub due_at: Option<OffsetDateTime>,
}

/// `None` = leave unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateRequestRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<RequestPriority>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub due_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignRequestRequest {
    pub assignee_user_id: UserId,
}
