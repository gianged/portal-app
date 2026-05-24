use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(pub Uuid);

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDto {
    pub id: UserId,
    pub name: String,
    pub email: String,
    pub role: UserRole,
    pub group_name: Option<String>,
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
