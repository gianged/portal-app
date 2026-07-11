use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    error::TransitionError,
    ids::{ServiceAccountId, UserId},
};

/// Admin-issued API principal for external read-only scripts. Carries only the
/// SHA-256 of its key; the plaintext is shown once at creation, never stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAccount {
    pub id: ServiceAccountId,
    pub name: String,
    /// SHA-256 of the `pak_*` secret.
    pub key_hash: Vec<u8>,
    pub status: ServiceAccountStatus,
    pub created_by: UserId,
    /// Set when the account transitions into `Revoked`; `None` otherwise.
    pub revoked_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAccountStatus {
    Active,
    Revoked,
}

impl ServiceAccountStatus {
    pub const fn try_revoke(self) -> Result<Self, TransitionError> {
        match self {
            Self::Active => Ok(Self::Revoked),
            Self::Revoked => Err(TransitionError::invalid("revoked", "revoked")),
        }
    }
}

impl ServiceAccount {
    pub fn revoke(&mut self, now: OffsetDateTime) -> Result<(), TransitionError> {
        self.status = self.status.try_revoke()?;
        self.revoked_at = Some(now);
        self.updated_at = now;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn revoke_only_once() {
        let t0 = OffsetDateTime::UNIX_EPOCH;
        let mut account = ServiceAccount {
            id: ServiceAccountId(Uuid::nil()),
            name: "reporting-script".to_owned(),
            key_hash: vec![0; 32],
            status: ServiceAccountStatus::Active,
            created_by: UserId(Uuid::nil()),
            revoked_at: None,
            created_at: t0,
            updated_at: t0,
        };
        account.revoke(t0).unwrap();
        assert_eq!(account.status, ServiceAccountStatus::Revoked);
        assert_eq!(account.revoked_at, Some(t0));
        assert!(account.revoke(t0).is_err());
    }
}
