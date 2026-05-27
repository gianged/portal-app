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

    /// Revoke does not set `responded_by_user_id` / `responded_at` — the schema
    /// CHECK only requires those for `accepted` / `declined`.
    pub fn revoke(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_revoke()?;
        self.updated_at = now;
        Ok(())
    }
}
