use std::fmt::Display;

use async_trait::async_trait;
use redis::{AsyncCommands, Client, aio::ConnectionManager};

use domain::{error::RepositoryError, ids::UserId, ports::presence::Presence};

/// `SETEX`-backed presence store; every key carries a TTL so a crashed process stops showing online.
#[derive(Clone)]
pub struct PresenceStore {
    conn: ConnectionManager,
}

impl PresenceStore {
    pub async fn new(url: &str) -> Result<Self, RepositoryError> {
        let client = Client::open(url).map_err(backend)?;
        let conn = ConnectionManager::new(client).await.map_err(backend)?;
        Ok(Self { conn })
    }
}

#[async_trait]
impl Presence for PresenceStore {
    /// Mark the user online for `ttl_secs` seconds; call again before expiry to heartbeat.
    #[tracing::instrument(skip_all, fields(user = ?user, ttl_secs = ?ttl_secs))]
    async fn set_online(&self, user: UserId, ttl_secs: u64) -> Result<(), RepositoryError> {
        let key = presence_key(user);
        let mut conn = self.conn.clone();
        conn.set_ex::<_, _, ()>(key, 1u8, ttl_secs)
            .await
            .map_err(backend)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(user = ?user))]
    async fn is_online(&self, user: UserId) -> Result<bool, RepositoryError> {
        let key = presence_key(user);
        let mut conn = self.conn.clone();
        conn.exists::<_, bool>(key).await.map_err(backend)
    }
}

fn presence_key(user: UserId) -> String {
    format!("portal:presence:user:{}", user.0)
}

fn backend<E: Display>(e: E) -> RepositoryError {
    RepositoryError::Backend(e.to_string())
}
