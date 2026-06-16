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
    /// Set when the request transitions into `Completed`; `None` otherwise.
    pub completed_at: Option<OffsetDateTime>,
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

    /// Reject a request currently in review back to `in_progress`.
    pub const fn try_reject(self) -> Result<Self, TransitionError> {
        match self {
            Self::Review => Ok(Self::InProgress),
            Self::Draft
            | Self::Submitted
            | Self::Assigned
            | Self::InProgress
            | Self::Completed
            | Self::Cancelled => Err(TransitionError::invalid(self.as_str(), "in_progress")),
        }
    }

    pub const fn try_cancel(self) -> Result<Self, TransitionError> {
        match self {
            Self::Draft | Self::Submitted | Self::Assigned | Self::InProgress | Self::Review => {
                Ok(Self::Cancelled)
            }
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
    pub fn assign(&mut self, assignee: UserId, now: OffsetDateTime) -> Result<(), TransitionError> {
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
        self.completed_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    pub fn reject(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_reject()?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;
    use uuid::Uuid;

    fn request(status: RequestStatus) -> Request {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        Request {
            id: RequestId(Uuid::nil()),
            project_id: ProjectId(Uuid::nil()),
            creator_user_id: UserId(Uuid::nil()),
            assignee_user_id: None,
            title: "Provision laptop".to_owned(),
            description: String::new(),
            status,
            priority: RequestPriority::Normal,
            due_at: None,
            completed_at: None,
            created_at: t0,
            updated_at: t0,
        }
    }

    #[test]
    fn full_lifecycle_happy_path() {
        assert_eq!(
            RequestStatus::Draft.try_submit().unwrap(),
            RequestStatus::Submitted
        );
        assert_eq!(
            RequestStatus::Submitted.try_assign().unwrap(),
            RequestStatus::Assigned
        );
        assert_eq!(
            RequestStatus::Assigned.try_start().unwrap(),
            RequestStatus::InProgress
        );
        assert_eq!(
            RequestStatus::InProgress.try_review().unwrap(),
            RequestStatus::Review
        );
        assert_eq!(
            RequestStatus::Review.try_complete().unwrap(),
            RequestStatus::Completed
        );
    }

    #[test]
    fn illegal_jumps_are_rejected() {
        assert!(RequestStatus::Draft.try_assign().is_err());
        assert!(RequestStatus::Draft.try_start().is_err());
        assert!(RequestStatus::Submitted.try_start().is_err());
        assert!(RequestStatus::Assigned.try_review().is_err());
        assert!(RequestStatus::Completed.try_submit().is_err());
    }

    #[test]
    fn reject_returns_review_to_in_progress() {
        assert_eq!(
            RequestStatus::Review.try_reject().unwrap(),
            RequestStatus::InProgress
        );
        assert!(RequestStatus::InProgress.try_reject().is_err());
    }

    #[test]
    fn cancel_allowed_pre_terminal_only() {
        for s in [
            RequestStatus::Draft,
            RequestStatus::Submitted,
            RequestStatus::Assigned,
            RequestStatus::InProgress,
            RequestStatus::Review,
        ] {
            assert_eq!(s.try_cancel().unwrap(), RequestStatus::Cancelled);
        }
        assert!(RequestStatus::Completed.try_cancel().is_err());
        assert!(RequestStatus::Cancelled.try_cancel().is_err());
    }

    #[test]
    fn assign_populates_assignee() {
        let assignee = UserId(Uuid::from_u128(7));
        let t1 = OffsetDateTime::UNIX_EPOCH + Duration::hours(1);
        let mut r = request(RequestStatus::Submitted);
        r.assign(assignee, t1).unwrap();
        assert_eq!(r.status, RequestStatus::Assigned);
        assert_eq!(r.assignee_user_id, Some(assignee));
        assert_eq!(r.updated_at, t1);
    }
}
