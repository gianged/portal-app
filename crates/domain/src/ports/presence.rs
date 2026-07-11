use std::time::Duration;

use async_trait::async_trait;

use crate::{error::PresenceError, ids::UserId};

/// Ephemeral online-presence tracking. Backed by a TTL'd key store in
/// `infrastructure`; the server's chat WebSocket refreshes a user's presence on
/// connect and on every heartbeat, and lets it lapse when the socket drops.
#[async_trait]
pub trait Presence: Send + Sync {
    /// Mark `user` online for `ttl`. Call again before the TTL elapses to keep
    /// the user present (heartbeat).
    async fn set_online(&self, user: UserId, ttl: Duration) -> Result<(), PresenceError>;

    /// Whether `user` currently has a live presence key.
    async fn is_online(&self, user: UserId) -> Result<bool, PresenceError>;
}
