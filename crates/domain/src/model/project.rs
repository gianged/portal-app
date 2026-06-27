use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    error::TransitionError,
    ids::{GroupId, ProjectCollaboratorId, ProjectId, ProjectInviteId, UserId},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub owner_group_id: GroupId,
    pub created_by_user_id: UserId,
    pub name: String,
    pub description: String,
    pub status: ProjectStatus,
    /// Completion percentage (0-100), set manually by group leaders. Validated
    /// at the `shared`/DB boundary; the domain trusts the value it receives.
    pub progress: u8,
    /// Set when the project transitions into `Completed`; `None` otherwise.
    pub completed_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Planning,
    Active,
    OnHold,
    Completed,
    Cancelled,
}

impl ProjectStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Planning => "planning",
            Self::Active => "active",
            Self::OnHold => "on_hold",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    pub const fn try_activate(self) -> Result<Self, TransitionError> {
        match self {
            Self::Planning => Ok(Self::Active),
            Self::Active | Self::OnHold | Self::Completed | Self::Cancelled => {
                Err(TransitionError::invalid(self.as_str(), "active"))
            }
        }
    }

    pub const fn try_hold(self) -> Result<Self, TransitionError> {
        match self {
            Self::Active => Ok(Self::OnHold),
            Self::Planning | Self::OnHold | Self::Completed | Self::Cancelled => {
                Err(TransitionError::invalid(self.as_str(), "on_hold"))
            }
        }
    }

    pub const fn try_resume(self) -> Result<Self, TransitionError> {
        match self {
            Self::OnHold => Ok(Self::Active),
            Self::Planning | Self::Active | Self::Completed | Self::Cancelled => {
                Err(TransitionError::invalid(self.as_str(), "active"))
            }
        }
    }

    pub const fn try_complete(self) -> Result<Self, TransitionError> {
        match self {
            Self::Active => Ok(Self::Completed),
            Self::Planning | Self::OnHold | Self::Completed | Self::Cancelled => {
                Err(TransitionError::invalid(self.as_str(), "completed"))
            }
        }
    }

    pub const fn try_cancel(self) -> Result<Self, TransitionError> {
        match self {
            Self::Planning | Self::Active | Self::OnHold => Ok(Self::Cancelled),
            Self::Completed | Self::Cancelled => {
                Err(TransitionError::invalid(self.as_str(), "cancelled"))
            }
        }
    }
}

impl Project {
    pub fn activate(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_activate()?;
        self.updated_at = now;
        Ok(())
    }

    pub fn hold(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_hold()?;
        self.updated_at = now;
        Ok(())
    }

    pub fn resume(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_resume()?;
        self.updated_at = now;
        Ok(())
    }

    pub fn complete(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_complete()?;
        self.completed_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    pub fn cancel(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_cancel()?;
        self.updated_at = now;
        Ok(())
    }

    /// Set the manual completion percentage. Range is enforced at the
    /// `shared`/DB boundary; here we only record the value and bump `updated_at`.
    pub fn set_progress(&mut self, progress: u8, now: OffsetDateTime) {
        self.progress = progress;
        self.updated_at = now;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCollaborator {
    pub id: ProjectCollaboratorId,
    pub project_id: ProjectId,
    pub group_id: GroupId,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInvite {
    pub id: ProjectInviteId,
    pub project_id: ProjectId,
    pub invited_by_user_id: UserId,
    pub invited_group_id: GroupId,
    pub responded_by_user_id: Option<UserId>,
    pub status: ProjectInviteStatus,
    pub responded_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectInviteStatus {
    Pending,
    Accepted,
    Declined,
    Revoked,
}

impl ProjectInviteStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Revoked => "revoked",
        }
    }

    pub const fn try_accept(self) -> Result<Self, TransitionError> {
        match self {
            Self::Pending => Ok(Self::Accepted),
            Self::Accepted | Self::Declined | Self::Revoked => {
                Err(TransitionError::invalid(self.as_str(), "accepted"))
            }
        }
    }

    pub const fn try_decline(self) -> Result<Self, TransitionError> {
        match self {
            Self::Pending => Ok(Self::Declined),
            Self::Accepted | Self::Declined | Self::Revoked => {
                Err(TransitionError::invalid(self.as_str(), "declined"))
            }
        }
    }

    pub const fn try_revoke(self) -> Result<Self, TransitionError> {
        match self {
            Self::Pending => Ok(Self::Revoked),
            Self::Accepted | Self::Declined | Self::Revoked => {
                Err(TransitionError::invalid(self.as_str(), "revoked"))
            }
        }
    }
}

impl ProjectInvite {
    pub fn accept(
        &mut self,
        responder: UserId,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        self.status = self.status.try_accept()?;
        self.responded_by_user_id = Some(responder);
        self.responded_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    pub fn decline(
        &mut self,
        responder: UserId,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        self.status = self.status.try_decline()?;
        self.responded_by_user_id = Some(responder);
        self.responded_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    /// Revoke does not set `responded_by_user_id` / `responded_at`; the schema CHECK
    /// only requires those for `accepted` / `declined`.
    pub fn revoke(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_revoke()?;
        self.updated_at = now;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    #[test]
    fn activate_only_from_planning() {
        assert_eq!(
            ProjectStatus::Planning.try_activate().unwrap(),
            ProjectStatus::Active
        );
        for s in [
            ProjectStatus::Active,
            ProjectStatus::OnHold,
            ProjectStatus::Completed,
            ProjectStatus::Cancelled,
        ] {
            assert!(s.try_activate().is_err(), "{s:?} should not activate");
        }
    }

    #[test]
    fn hold_and_resume_round_trip() {
        let held = ProjectStatus::Active.try_hold().unwrap();
        assert_eq!(held, ProjectStatus::OnHold);
        assert_eq!(held.try_resume().unwrap(), ProjectStatus::Active);
        // resume is only valid from on_hold
        assert!(ProjectStatus::Active.try_resume().is_err());
    }

    #[test]
    fn complete_only_from_active() {
        assert_eq!(
            ProjectStatus::Active.try_complete().unwrap(),
            ProjectStatus::Completed
        );
        assert!(ProjectStatus::Planning.try_complete().is_err());
        assert!(ProjectStatus::OnHold.try_complete().is_err());
    }

    #[test]
    fn cancel_allowed_pre_terminal_only() {
        for s in [
            ProjectStatus::Planning,
            ProjectStatus::Active,
            ProjectStatus::OnHold,
        ] {
            assert_eq!(s.try_cancel().unwrap(), ProjectStatus::Cancelled);
        }
        assert!(ProjectStatus::Completed.try_cancel().is_err());
        assert!(ProjectStatus::Cancelled.try_cancel().is_err());
    }

    #[test]
    fn activate_sets_updated_at() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let t1 = t0 + Duration::hours(3);
        let mut p = Project {
            id: ProjectId(uuid::Uuid::nil()),
            owner_group_id: GroupId(uuid::Uuid::nil()),
            created_by_user_id: UserId(uuid::Uuid::nil()),
            name: "Helios".to_owned(),
            description: String::new(),
            status: ProjectStatus::Planning,
            progress: 0,
            completed_at: None,
            created_at: t0,
            updated_at: t0,
        };
        p.activate(t1).unwrap();
        assert_eq!(p.status, ProjectStatus::Active);
        assert_eq!(p.updated_at, t1);
    }

    #[test]
    fn complete_sets_completed_at() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let t1 = t0 + Duration::hours(5);
        let mut p = Project {
            id: ProjectId(uuid::Uuid::nil()),
            owner_group_id: GroupId(uuid::Uuid::nil()),
            created_by_user_id: UserId(uuid::Uuid::nil()),
            name: "Helios".to_owned(),
            description: String::new(),
            status: ProjectStatus::Active,
            progress: 80,
            completed_at: None,
            created_at: t0,
            updated_at: t0,
        };
        p.complete(t1).unwrap();
        assert_eq!(p.status, ProjectStatus::Completed);
        assert_eq!(p.completed_at, Some(t1));
    }

    #[test]
    fn invite_status_transitions() {
        assert_eq!(
            ProjectInviteStatus::Pending.try_accept().unwrap(),
            ProjectInviteStatus::Accepted
        );
        assert_eq!(
            ProjectInviteStatus::Pending.try_decline().unwrap(),
            ProjectInviteStatus::Declined
        );
        assert_eq!(
            ProjectInviteStatus::Pending.try_revoke().unwrap(),
            ProjectInviteStatus::Revoked
        );
        // A settled invite cannot transition again.
        assert!(ProjectInviteStatus::Accepted.try_decline().is_err());
        assert!(ProjectInviteStatus::Declined.try_revoke().is_err());
        assert!(ProjectInviteStatus::Revoked.try_accept().is_err());
    }

    #[test]
    fn invite_accept_records_responder() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let responder = UserId(uuid::Uuid::nil());
        let mut invite = ProjectInvite {
            id: ProjectInviteId(uuid::Uuid::nil()),
            project_id: ProjectId(uuid::Uuid::nil()),
            invited_by_user_id: UserId(uuid::Uuid::nil()),
            invited_group_id: GroupId(uuid::Uuid::nil()),
            responded_by_user_id: None,
            status: ProjectInviteStatus::Pending,
            responded_at: None,
            created_at: t0,
            updated_at: t0,
        };
        invite.accept(responder, t0 + Duration::minutes(2)).unwrap();
        assert_eq!(invite.status, ProjectInviteStatus::Accepted);
        assert_eq!(invite.responded_by_user_id, Some(responder));
        assert!(invite.responded_at.is_some());
    }
}
