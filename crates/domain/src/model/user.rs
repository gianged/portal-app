use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{error::TransitionError, ids::UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub password_hash: String,
    pub full_name: String,
    pub avatar_storage_key: Option<String>,
    pub phone: Option<String>,
    pub timezone: String,
    pub status: UserStatus,
    pub system_role: Option<SystemRole>,
    /// Opt-out switch for the email notification side-channel.
    pub email_notifications: bool,
    pub first_logged_in_at: Option<OffsetDateTime>,
    pub deactivated_at: Option<OffsetDateTime>,
    /// Optimistic-lock version as loaded; the guarded save bumps it.
    pub version: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    Pending,
    Active,
    Deactivated,
}

/// Org-wide identity, orthogonal to per-group `GroupRole`. Most users have `None`;
/// only Directors and HR carry one. IT staff are members of the `GroupKind::It` group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemRole {
    Director,
    Hr,
}

impl UserStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Deactivated => "deactivated",
        }
    }

    pub const fn try_activate(self) -> Result<Self, TransitionError> {
        match self {
            Self::Pending => Ok(Self::Active),
            Self::Active | Self::Deactivated => {
                Err(TransitionError::invalid(self.as_str(), "active"))
            }
        }
    }

    pub const fn try_deactivate(self) -> Result<Self, TransitionError> {
        match self {
            Self::Active => Ok(Self::Deactivated),
            Self::Pending | Self::Deactivated => {
                Err(TransitionError::invalid(self.as_str(), "deactivated"))
            }
        }
    }

    pub const fn try_reactivate(self) -> Result<Self, TransitionError> {
        match self {
            Self::Deactivated => Ok(Self::Active),
            Self::Pending | Self::Active => Err(TransitionError::invalid(self.as_str(), "active")),
        }
    }
}

impl User {
    /// Marks a pending user as active on first login, recording `first_logged_in_at`.
    /// Only valid on a `Pending` user; use `reactivate` for a deactivated one.
    pub fn activate(
        &mut self,
        first_logged_in_at: OffsetDateTime,
        now: OffsetDateTime,
    ) -> Result<(), TransitionError> {
        self.status = self.status.try_activate()?;
        self.first_logged_in_at = Some(first_logged_in_at);
        self.updated_at = now;
        Ok(())
    }

    pub fn deactivate(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_deactivate()?;
        self.deactivated_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    pub fn reactivate(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_reactivate()?;
        self.deactivated_at = None;
        self.updated_at = now;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;
    use uuid::Uuid;

    fn user(status: UserStatus) -> User {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        User {
            id: UserId(Uuid::nil()),
            email: "jane@example.com".to_owned(),
            password_hash: "x".to_owned(),
            full_name: "Jane".to_owned(),
            avatar_storage_key: None,
            phone: None,
            timezone: "UTC".to_owned(),
            status,
            system_role: None,
            email_notifications: true,
            first_logged_in_at: None,
            deactivated_at: None,
            version: 0,
            created_at: t0,
            updated_at: t0,
        }
    }

    #[test]
    fn status_transitions() {
        assert_eq!(
            UserStatus::Pending.try_activate().unwrap(),
            UserStatus::Active
        );
        assert_eq!(
            UserStatus::Active.try_deactivate().unwrap(),
            UserStatus::Deactivated
        );
        assert_eq!(
            UserStatus::Deactivated.try_reactivate().unwrap(),
            UserStatus::Active
        );
        // Illegal transitions.
        assert!(UserStatus::Pending.try_deactivate().is_err());
        assert!(UserStatus::Pending.try_reactivate().is_err());
        assert!(UserStatus::Active.try_activate().is_err());
        assert!(UserStatus::Deactivated.try_activate().is_err());
    }

    #[test]
    fn activate_sets_first_login() {
        let login = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        let mut u = user(UserStatus::Pending);
        u.activate(login, login).unwrap();
        assert_eq!(u.status, UserStatus::Active);
        assert_eq!(u.first_logged_in_at, Some(login));
    }

    #[test]
    fn deactivate_then_reactivate_round_trips_deactivated_at() {
        let t1 = OffsetDateTime::UNIX_EPOCH + Duration::days(1);
        let mut u = user(UserStatus::Active);
        u.deactivate(t1).unwrap();
        assert_eq!(u.status, UserStatus::Deactivated);
        assert_eq!(u.deactivated_at, Some(t1));

        u.reactivate(t1 + Duration::days(1)).unwrap();
        assert_eq!(u.status, UserStatus::Active);
        assert_eq!(u.deactivated_at, None);
    }
}
