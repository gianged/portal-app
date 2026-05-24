use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    error::TransitionError,
    ids::{TicketId, UserId},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    pub id: TicketId,
    pub requester_user_id: UserId,
    pub assignee_user_id: Option<UserId>,
    pub title: String,
    pub description: String,
    pub status: TicketStatus,
    pub priority: Option<TicketPriority>,
    pub category: TicketCategory,
    pub triaged_at: Option<OffsetDateTime>,
    pub resolved_at: Option<OffsetDateTime>,
    pub closed_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Open,
    Triaged,
    Assigned,
    InProgress,
    Resolved,
    Closed,
    Reopened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketPriority {
    Low,
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketCategory {
    Hardware,
    Software,
    Access,
    Other,
}

impl TicketStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Triaged => "triaged",
            Self::Assigned => "assigned",
            Self::InProgress => "in_progress",
            Self::Resolved => "resolved",
            Self::Closed => "closed",
            Self::Reopened => "reopened",
        }
    }

    pub const fn try_triage(self) -> Result<Self, TransitionError> {
        match self {
            Self::Open | Self::Reopened => Ok(Self::Triaged),
            Self::Triaged
            | Self::Assigned
            | Self::InProgress
            | Self::Resolved
            | Self::Closed => Err(TransitionError::invalid(self.as_str(), "triaged")),
        }
    }

    pub const fn try_assign(self) -> Result<Self, TransitionError> {
        match self {
            Self::Triaged => Ok(Self::Assigned),
            Self::Open
            | Self::Assigned
            | Self::InProgress
            | Self::Resolved
            | Self::Closed
            | Self::Reopened => Err(TransitionError::invalid(self.as_str(), "assigned")),
        }
    }

    pub const fn try_start(self) -> Result<Self, TransitionError> {
        match self {
            Self::Assigned => Ok(Self::InProgress),
            Self::Open
            | Self::Triaged
            | Self::InProgress
            | Self::Resolved
            | Self::Closed
            | Self::Reopened => Err(TransitionError::invalid(self.as_str(), "in_progress")),
        }
    }

    pub const fn try_resolve(self) -> Result<Self, TransitionError> {
        match self {
            Self::InProgress => Ok(Self::Resolved),
            Self::Open
            | Self::Triaged
            | Self::Assigned
            | Self::Resolved
            | Self::Closed
            | Self::Reopened => Err(TransitionError::invalid(self.as_str(), "resolved")),
        }
    }

    pub const fn try_close(self) -> Result<Self, TransitionError> {
        match self {
            Self::Resolved => Ok(Self::Closed),
            Self::Open
            | Self::Triaged
            | Self::Assigned
            | Self::InProgress
            | Self::Closed
            | Self::Reopened => Err(TransitionError::invalid(self.as_str(), "closed")),
        }
    }

    /// 7-day reopen window is enforced in `application`, not here.
    pub const fn try_reopen(self) -> Result<Self, TransitionError> {
        match self {
            Self::Closed => Ok(Self::Reopened),
            Self::Open
            | Self::Triaged
            | Self::Assigned
            | Self::InProgress
            | Self::Resolved
            | Self::Reopened => Err(TransitionError::invalid(self.as_str(), "reopened")),
        }
    }
}

impl Ticket {
    /// Triage requires a priority; encodes the schema CHECK
    /// "priority IS NOT NULL once status != 'open'".
    pub fn triage(
        &mut self,
        priority: TicketPriority,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        self.status = self.status.try_triage()?;
        self.priority = Some(priority);
        if self.triaged_at.is_none() {
            self.triaged_at = Some(now);
        }
        self.updated_at = now;
        Ok(())
    }

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

    pub fn resolve(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_resolve()?;
        self.resolved_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    pub fn close(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_close()?;
        self.closed_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    pub fn reopen(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_reopen()?;
        self.closed_at = None;
        self.resolved_at = None;
        self.updated_at = now;
        Ok(())
    }
}
