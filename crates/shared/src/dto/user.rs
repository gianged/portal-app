use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{
    group::GroupRole,
    ids::{GroupId, UserId},
};

/// Synthetic display role shown in the UI. This is a presentation flattening of
/// the domain's split identity (`SystemRole` + per-group `GroupRole` +
/// `GroupKind::It`); the server decides which single role to surface per view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Director,
    Hr,
    GroupLeader,
    GroupSubLeader,
    Member,
    It,
}

impl UserRole {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Director => "Director",
            Self::Hr => "HR",
            Self::GroupLeader => "Group Leader",
            Self::GroupSubLeader => "Sub-leader",
            Self::Member => "Member",
            Self::It => "IT",
        }
    }
}

/// Account lifecycle state. Mirrors `domain::model::UserStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    Pending,
    Active,
    Deactivated,
}

impl UserStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Active => "Active",
            Self::Deactivated => "Deactivated",
        }
    }
}

/// Org-wide identity. Mirrors `domain::model::SystemRole` (IT is not here; IT
/// staff are members of the `GroupKind::It` group).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemRole {
    Director,
    Hr,
}

impl SystemRole {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Director => "Director",
            Self::Hr => "HR",
        }
    }
}

/// One active group membership on the wire user; lets the client evaluate
/// per-group leadership exactly instead of trusting the flattened `role`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMembershipDto {
    pub group_id: GroupId,
    pub group_name: String,
    pub role: GroupRole,
}

/// Lightweight user shape for lists and the auth response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDto {
    pub id: UserId,
    pub name: String,
    pub email: String,
    pub role: UserRole,
    pub memberships: Vec<UserMembershipDto>,
}

/// Fuller account view for the profile / admin detail screen. Secrets
/// (`password_hash`) and bookkeeping timestamps are omitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfileDto {
    pub id: UserId,
    pub email: String,
    pub full_name: String,
    pub avatar_storage_key: Option<String>,
    pub phone: Option<String>,
    pub timezone: String,
    pub status: UserStatus,
    pub system_role: Option<SystemRole>,
    /// Opt-out switch for the email notification side-channel.
    pub email_notifications: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub user: UserDto,
}

/// Maps to `application::commands::CreateUserCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub password: String,
    pub full_name: String,
    pub phone: Option<String>,
    pub timezone: String,
    pub system_role: Option<SystemRole>,
}

/// Maps to `application::commands::UpdateProfileCommand`. `None` = leave
/// unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateProfileRequest {
    pub full_name: Option<String>,
    pub phone: Option<String>,
    pub timezone: Option<String>,
    pub avatar_storage_key: Option<String>,
    pub email_notifications: Option<bool>,
}

/// Self-service password change: the current password is re-verified before
/// the new one is accepted, and every existing session is revoked on success.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// HR-set temporary password for another user (revokes the target's sessions).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetPasswordRequest {
    pub new_password: String,
}

/// Assign a user to a group with an initial role (HR action).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignUserToGroupRequest {
    pub group_id: GroupId,
    pub role: GroupRole,
}

/// Change a user's org-wide role (HR action). `None` clears it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSystemRoleRequest {
    pub system_role: Option<SystemRole>,
}

#[cfg(test)]
mod tests {
    use super::{SystemRole, UserRole, UserStatus};

    #[test]
    fn roles_serialize_snake_case() {
        assert_eq!(
            serde_json::to_string(&UserRole::GroupLeader).unwrap(),
            "\"group_leader\""
        );
        assert_eq!(serde_json::to_string(&UserRole::It).unwrap(), "\"it\"");
        assert_eq!(
            serde_json::to_string(&UserStatus::Deactivated).unwrap(),
            "\"deactivated\""
        );
        assert_eq!(serde_json::to_string(&SystemRole::Hr).unwrap(), "\"hr\"");
    }

    #[test]
    fn role_round_trips() {
        let json = serde_json::to_string(&UserRole::GroupSubLeader).unwrap();
        let back: UserRole = serde_json::from_str(&json).unwrap();
        assert_eq!(back, UserRole::GroupSubLeader);
    }
}
