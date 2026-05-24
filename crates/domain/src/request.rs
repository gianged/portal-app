use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    error::TransitionError,
    ids::{ProjectId, RequestAttachmentId, RequestId, UserId},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: RequestId,
    pub project_id: ProjectId,
    pub creator_user_id: UserId,
    pub assignee_user_id: Option<UserId>,
    pub title: String,
    pub description: String,
    pub status: RequestStatus,
    pub priority: RequestPriority,
    pub due_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestPriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl RequestStatus {
    const fn as_str(self) -> &'static str {
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

    pub const fn try_submit(self) -> Result<Self, TransitionError> {
        match self {
            Self::Draft => Ok(Self::Submitted),
            Self::Submitted
            | Self::Assigned
            | Self::InProgress
            | Self::Review
            | Self::Completed
            | Self::Cancelled => Err(TransitionError::invalid(self.as_str(), "submitted")),
        }
    }

    pub const fn try_assign(self) -> Result<Self, TransitionError> {
        match self {
            Self::Submitted => Ok(Self::Assigned),
            Self::Draft
            | Self::Assigned
            | Self::InProgress
            | Self::Review
            | Self::Completed
            | Self::Cancelled => Err(TransitionError::invalid(self.as_str(), "assigned")),
        }
    }

    pub const fn try_start(self) -> Result<Self, TransitionError> {
        match self {
            Self::Assigned => Ok(Self::InProgress),
            Self::Draft
            | Self::Submitted
            | Self::InProgress
            | Self::Review
            | Self::Completed
            | Self::Cancelled => Err(TransitionError::invalid(self.as_str(), "in_progress")),
        }
    }

    pub const fn try_review(self) -> Result<Self, TransitionError> {
        match self {
            Self::InProgress => Ok(Self::Review),
            Self::Draft
            | Self::Submitted
            | Self::Assigned
            | Self::Review
            | Self::Completed
            | Self::Cancelled => Err(TransitionError::invalid(self.as_str(), "review")),
        }
    }

    pub const fn try_complete(self) -> Result<Self, TransitionError> {
        match self {
            Self::Review => Ok(Self::Completed),
            Self::Draft
            | Self::Submitted
            | Self::Assigned
            | Self::InProgress
            | Self::Completed
            | Self::Cancelled => Err(TransitionError::invalid(self.as_str(), "completed")),
        }
    }

    pub const fn try_cancel(self) -> Result<Self, TransitionError> {
        match self {
            Self::Draft
            | Self::Submitted
            | Self::Assigned
            | Self::InProgress
            | Self::Review => Ok(Self::Cancelled),
            Self::Completed | Self::Cancelled => {
                Err(TransitionError::invalid(self.as_str(), "cancelled"))
            }
        }
    }
}

impl Request {
    pub fn submit(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_submit()?;
        self.updated_at = now;
        Ok(())
    }

    /// The only path past `Submitted`; encodes the schema invariant
    /// "`assignee_user_id` required once status > submitted".
    pub fn assign(
        &mut self,
        assignee: UserId,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        self.status = self.status.try_assign()?;
        self.assignee_user_id = Some(assignee);
        self.updated_at = now;
        Ok(())
    }

    pub fn start(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_start()?;
        self.updated_at = now;
        Ok(())
    }

    pub fn send_for_review(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_review()?;
        self.updated_at = now;
        Ok(())
    }

    pub fn complete(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_complete()?;
        self.updated_at = now;
        Ok(())
    }

    pub fn cancel(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_cancel()?;
        self.updated_at = now;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestAttachment {
    pub id: RequestAttachmentId,
    pub request_id: RequestId,
    pub uploaded_by_user_id: UserId,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub storage_key: String,
    pub created_at: OffsetDateTime,
}
