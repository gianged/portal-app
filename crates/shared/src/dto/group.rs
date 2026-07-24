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
    /// Every variant, for building select options.
    pub const ALL: [Self; 2] = [Self::Standard, Self::It];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::It => "IT",
        }
    }

    /// Canonical wire string (the serde `snake_case` tag).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::It => "it",
        }
    }

    /// Parses a wire string produced by [`Self::as_str`].
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
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
    /// Every variant, for building select options.
    pub const ALL: [Self; 3] = [Self::Leader, Self::SubLeader, Self::Member];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Leader => "Leader",
            Self::SubLeader => "Sub-leader",
            Self::Member => "Member",
        }
    }

    /// Canonical wire string (the serde `snake_case` tag).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Leader => "leader",
            Self::SubLeader => "sub_leader",
            Self::Member => "member",
        }
    }

    /// Parses a wire string produced by [`Self::as_str`].
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
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
    /// True on a create response whose authz grant is still being reconciled;
    /// permissions may lag briefly. Always false on reads.
    #[serde(default)]
    pub authz_pending: bool,
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
    /// True on a create response whose authz grant is still being reconciled;
    /// permissions may lag briefly. Always false on reads.
    #[serde(default)]
    pub authz_pending: bool,
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

/// Hands the single leader slot from `from_user_id` to `to_user_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferLeadershipRequest {
    pub from_user_id: UserId,
    pub to_user_id: UserId,
}

#[cfg(test)]
mod tests {
    use super::{GroupKind, GroupRole};

    #[test]
    fn wire_helpers_match_serde() {
        for k in GroupKind::ALL {
            assert_eq!(
                serde_json::to_string(&k).unwrap(),
                format!("\"{}\"", k.as_str())
            );
            assert_eq!(GroupKind::from_wire(k.as_str()), Some(k));
        }
        for r in GroupRole::ALL {
            assert_eq!(
                serde_json::to_string(&r).unwrap(),
                format!("\"{}\"", r.as_str())
            );
            assert_eq!(GroupRole::from_wire(r.as_str()), Some(r));
        }
    }
}
