use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{
    common::UserSummaryDto,
    ids::{GroupId, MembershipId, UserId},
};

/// Mirrors `domain::model::GroupKind`. IT is a special-purpose group, not a
/// per-user identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    Standard,
    It,
}

impl GroupKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::It => "IT",
        }
    }
}

/// A user's role within a single group. Mirrors `domain::model::GroupRole`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    Leader,
    SubLeader,
    Member,
}

impl GroupRole {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Leader => "Leader",
            Self::SubLeader => "Sub-leader",
            Self::Member => "Member",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupDto {
    pub id: GroupId,
    pub name: String,
    pub description: String,
    pub kind: GroupKind,
    pub member_count: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipDto {
    pub id: MembershipId,
    pub user: UserSummaryDto,
    pub role: GroupRole,
    #[serde(with = "time::serde::rfc3339")]
    pub joined_at: OffsetDateTime,
    /// `false` once the membership has been deactivated (derived from
    /// `deactivated_at`).
    pub active: bool,
}

/// Group header plus its roster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupDetailDto {
    pub group: GroupDto,
    pub members: Vec<MembershipDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: String,
    pub kind: GroupKind,
}

/// `None` = leave unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateGroupRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMemberRequest {
    pub user_id: UserId,
    pub role: GroupRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeMemberRoleRequest {
    pub role: GroupRole,
}
