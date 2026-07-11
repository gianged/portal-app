use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{
    common::{GroupSummaryDto, UserSummaryDto},
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
    /// Every variant in lifecycle order, for building select options.
    pub const ALL: [Self; 7] = [
        Self::Draft,
        Self::Submitted,
        Self::Assigned,
        Self::InProgress,
        Self::Review,
        Self::Completed,
        Self::Cancelled,
    ];

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

    /// Canonical wire string (the serde `snake_case` tag).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Submitted => "submitted",
            Self::Assigned => "assigned",
            Self::InProgress => "in_progress",
            Self::Review => "review",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    /// Parses a wire string produced by [`Self::as_str`].
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
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
    /// Every variant, for building select options.
    pub const ALL: [Self; 4] = [Self::Low, Self::Normal, Self::High, Self::Urgent];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Normal => "Normal",
            Self::High => "High",
            Self::Urgent => "Urgent",
        }
    }

    /// Canonical wire string (the serde `snake_case` tag).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }

    /// Parses a wire string produced by [`Self::as_str`].
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestAttachmentDto {
    pub id: RequestAttachmentId,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: u64,
    /// Presigned, time-limited URL the client uses to fetch the file.
    pub download_url: String,
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
    pub progress: u8,
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
    /// Owner group of the request's project; drives client-side gating of the
    /// leader-only lifecycle actions.
    pub owner_group: GroupSummaryDto,
    pub attachments: Vec<RequestAttachmentDto>,
}

/// Maps to `application::commands::CreateRequestCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRequestRequest {
    pub project_id: ProjectId,
    pub title: String,
    pub description: String,
    pub priority: RequestPriority,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub due_at: Option<OffsetDateTime>,
}

/// `None` = leave unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateRequestRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<RequestPriority>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub due_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignRequestRequest {
    pub assignee_user_id: UserId,
}

/// New completion percentage (0-100); validated server-side.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SetRequestProgressRequest {
    pub progress: u8,
}

#[cfg(test)]
mod tests {
    use super::{RequestPriority, RequestStatus};

    #[test]
    fn wire_helpers_match_serde() {
        for s in RequestStatus::ALL {
            assert_eq!(
                serde_json::to_string(&s).unwrap(),
                format!("\"{}\"", s.as_str())
            );
            assert_eq!(RequestStatus::from_wire(s.as_str()), Some(s));
        }
        for p in RequestPriority::ALL {
            assert_eq!(
                serde_json::to_string(&p).unwrap(),
                format!("\"{}\"", p.as_str())
            );
            assert_eq!(RequestPriority::from_wire(p.as_str()), Some(p));
        }
    }
}
