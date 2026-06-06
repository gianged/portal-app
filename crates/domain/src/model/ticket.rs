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
            Self::Triaged | Self::Assigned | Self::InProgress | Self::Resolved | Self::Closed => {
                Err(TransitionError::invalid(self.as_str(), "triaged"))
            }
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

    /// Requester rejects an IT resolution; ticket goes back to `in_progress`.
    pub const fn try_reject_resolution(self) -> Result<Self, TransitionError> {
        match self {
            Self::Resolved => Ok(Self::InProgress),
            Self::Open
            | Self::Triaged
            | Self::Assigned
            | Self::InProgress
            | Self::Closed
            | Self::Reopened => Err(TransitionError::invalid(self.as_str(), "in_progress")),
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

    pub fn reject_resolution(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_reject_resolution()?;
        self.resolved_at = None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{TicketId, UserId};
    use time::Duration;
    use uuid::Uuid;

    fn ticket(status: TicketStatus) -> Ticket {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        Ticket {
            id: TicketId(Uuid::nil()),
            requester_user_id: UserId(Uuid::nil()),
            assignee_user_id: None,
            title: "Printer down".to_owned(),
            description: "It is on fire".to_owned(),
            status,
            priority: None,
            category: TicketCategory::Hardware,
            triaged_at: None,
            resolved_at: None,
            closed_at: None,
            created_at: t0,
            updated_at: t0,
        }
    }

    #[test]
    fn triage_allowed_from_open_or_reopened_only() {
        assert_eq!(
            TicketStatus::Open.try_triage().unwrap(),
            TicketStatus::Triaged
        );
        assert_eq!(
            TicketStatus::Reopened.try_triage().unwrap(),
            TicketStatus::Triaged
        );
        for s in [
            TicketStatus::Triaged,
            TicketStatus::Assigned,
            TicketStatus::InProgress,
            TicketStatus::Resolved,
            TicketStatus::Closed,
        ] {
            assert!(s.try_triage().is_err(), "{s:?} should not triage");
        }
    }

    #[test]
    fn happy_path_open_to_closed() {
        assert_eq!(
            TicketStatus::Open.try_triage().unwrap(),
            TicketStatus::Triaged
        );
        assert_eq!(
            TicketStatus::Triaged.try_assign().unwrap(),
            TicketStatus::Assigned
        );
        assert_eq!(
            TicketStatus::Assigned.try_start().unwrap(),
            TicketStatus::InProgress
        );
        assert_eq!(
            TicketStatus::InProgress.try_resolve().unwrap(),
            TicketStatus::Resolved
        );
        assert_eq!(
            TicketStatus::Resolved.try_close().unwrap(),
            TicketStatus::Closed
        );
    }

    #[test]
    fn assign_requires_triaged() {
        assert!(TicketStatus::Open.try_assign().is_err());
        assert!(TicketStatus::Assigned.try_assign().is_err());
    }

    #[test]
    fn triage_sets_priority_and_timestamp_idempotently() {
        let t1 = OffsetDateTime::UNIX_EPOCH + Duration::hours(1);
        let t2 = t1 + Duration::hours(1);
        let mut t = ticket(TicketStatus::Open);
        t.triage(TicketPriority::High, t1).unwrap();
        assert_eq!(t.status, TicketStatus::Triaged);
        assert_eq!(t.priority, Some(TicketPriority::High));
        assert_eq!(t.triaged_at, Some(t1));

        // Re-triage after a reopen keeps the original triaged_at.
        t.status = TicketStatus::Reopened;
        t.triage(TicketPriority::Urgent, t2).unwrap();
        assert_eq!(t.triaged_at, Some(t1), "triaged_at must not be overwritten");
        assert_eq!(t.priority, Some(TicketPriority::Urgent));
    }

    #[test]
    fn resolve_then_reopen_clears_resolution_timestamps() {
        let t1 = OffsetDateTime::UNIX_EPOCH + Duration::hours(1);
        let mut t = ticket(TicketStatus::InProgress);
        t.resolve(t1).unwrap();
        assert_eq!(t.resolved_at, Some(t1));
        t.close(t1).unwrap();
        assert_eq!(t.closed_at, Some(t1));

        t.reopen(t1 + Duration::days(1)).unwrap();
        assert_eq!(t.status, TicketStatus::Reopened);
        assert_eq!(t.closed_at, None);
        assert_eq!(t.resolved_at, None);
    }

    #[test]
    fn reject_resolution_returns_to_in_progress_and_clears_resolved_at() {
        let t1 = OffsetDateTime::UNIX_EPOCH + Duration::hours(1);
        let mut t = ticket(TicketStatus::InProgress);
        t.resolve(t1).unwrap();
        t.reject_resolution(t1 + Duration::minutes(5)).unwrap();
        assert_eq!(t.status, TicketStatus::InProgress);
        assert_eq!(t.resolved_at, None);
    }

    #[test]
    fn reopen_requires_closed() {
        for s in [
            TicketStatus::Open,
            TicketStatus::Triaged,
            TicketStatus::Assigned,
            TicketStatus::InProgress,
            TicketStatus::Resolved,
            TicketStatus::Reopened,
        ] {
            assert!(s.try_reopen().is_err(), "{s:?} should not reopen");
        }
        assert_eq!(
            TicketStatus::Closed.try_reopen().unwrap(),
            TicketStatus::Reopened
        );
    }
}
