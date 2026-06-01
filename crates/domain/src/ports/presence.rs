use async_trait::async_trait;

use crate::{error::RepositoryError, ids::UserId};

/// Ephemeral online-presence tracking. Backed by a TTL'd key store in
/// `infrastructure`; the server's chat WebSocket refreshes a user's presence on
/// connect and on every heartbeat, and lets it lapse when the socket drops.
#[async_trait]
pub trait Presence: Send + Sync {
    /// Mark `user` online for `ttl_secs` seconds. Call again before the TTL
    /// elapses to keep the user present (heartbeat).
    async fn set_online(&self, user: UserId, ttl_secs: u64) -> Result<(), RepositoryError>;

    /// Whether `user` currently has a live presence key.
    async fn is_online(&self, user: UserId) -> Result<bool, RepositoryError>;
}
