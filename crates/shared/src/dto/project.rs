use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{
    common::{GroupSummaryDto, UserSummaryDto},
    ids::{GroupId, ProjectCollaboratorId, ProjectId, ProjectInviteId},
};

/// Mirrors `domain::model::ProjectStatus`.
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
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Planning => "Planning",
            Self::Active => "Active",
            Self::OnHold => "On Hold",
            Self::Completed => "Completed",
            Self::Cancelled => "Cancelled",
        }
    }
}

/// Mirrors `domain::model::ProjectInviteStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectInviteStatus {
    Pending,
    Accepted,
    Declined,
    Revoked,
}

impl ProjectInviteStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Accepted => "Accepted",
            Self::Declined => "Declined",
            Self::Revoked => "Revoked",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDto {
    pub id: ProjectId,
    pub owner_group: GroupSummaryDto,
    pub created_by: UserSummaryDto,
    pub name: String,
    pub description: String,
    pub status: ProjectStatus,
    /// Manual completion percentage (0-100), set by group leaders.
    pub progress: u8,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCollaboratorDto {
    pub id: ProjectCollaboratorId,
    pub group: GroupSummaryDto,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInviteDto {
    pub id: ProjectInviteId,
    pub project_id: ProjectId,
    pub invited_by: UserSummaryDto,
    pub invited_group: GroupSummaryDto,
    pub responded_by: Option<UserSummaryDto>,
    pub status: ProjectInviteStatus,
    #[serde(with = "time::serde::rfc3339::option")]
    pub responded_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Project header plus collaborators and any still-pending invites.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetailDto {
    pub project: ProjectDto,
    pub collaborators: Vec<ProjectCollaboratorDto>,
    pub pending_invites: Vec<ProjectInviteDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProjectRequest {
    pub owner_group_id: GroupId,
    pub name: String,
    pub description: String,
}

/// `None` = leave unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateProjectMetadataRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Target status; the server validates the transition against current state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeProjectStatusRequest {
    pub status: ProjectStatus,
}

/// New completion percentage (0-100); validated server-side.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SetProjectProgressRequest {
    pub progress: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteGroupRequest {
    pub group_id: GroupId,
}

/// `true` accepts, `false` declines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RespondInviteRequest {
    pub accept: bool,
}
