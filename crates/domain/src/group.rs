use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::ids::{GroupId, MembershipId, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    pub description: String,
    pub kind: GroupKind,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    Standard,
    It,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    pub id: MembershipId,
    pub group_id: GroupId,
    pub user_id: UserId,
    pub role: GroupRole,
    pub joined_at: OffsetDateTime,
    pub deactivated_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    Leader,
    SubLeader,
    Member,
}

impl Membership {
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.deactivated_at.is_none()
    }

    pub fn deactivate(&mut self, now: OffsetDateTime) {
        self.deactivated_at = Some(now);
        self.updated_at = now;
    }

    pub fn reactivate(&mut self, now: OffsetDateTime) {
        self.deactivated_at = None;
        self.updated_at = now;
    }

    pub fn change_role(&mut self, role: GroupRole, now: OffsetDateTime) {
        self.role = role;
        self.updated_at = now;
    }
}
