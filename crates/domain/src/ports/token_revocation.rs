use async_trait::async_trait;
use uuid::Uuid;

use crate::{error::RepositoryError, ids::UserId};

/// Server-side session-token invalidation: a denylist for individual tokens
/// (logout) plus a per-user version counter checked against the token's `ver`
/// claim, so bumping the version revokes every token a user holds at once
/// (deactivation, password change).
#[async_trait]
pub trait TokenRevocation: Send + Sync {
    /// Denylist a single token id for the remainder of its lifetime.
    async fn revoke(&self, jti: Uuid, ttl_secs: u64) -> Result<(), RepositoryError>;

    async fn is_revoked(&self, jti: Uuid) -> Result<bool, RepositoryError>;

    /// The user's current token version. Tokens minted with an older version
    /// are invalid. A user with no recorded version is at 0.
    async fn version(&self, user: UserId) -> Result<u64, RepositoryError>;

    /// Advance the user's token version, invalidating every outstanding token,
    /// and return the new version.
    async fn bump_version(&self, user: UserId) -> Result<u64, RepositoryError>;
}
