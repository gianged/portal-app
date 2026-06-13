use async_trait::async_trait;
use redis::{AsyncCommands, Client, Script, aio::ConnectionManager};
use uuid::Uuid;

use domain::{error::RepositoryError, ids::UserId, ports::token_revocation::TokenRevocation};

/// Redis-backed: per-token denylist entries that expire with the token, plus a
/// per-user version counter whose TTL refreshes on every read/bump.
#[derive(Clone)]
pub struct RedisTokenRevocation {
    conn: ConnectionManager,
    /// Version-key TTL - 2x the session TTL (set by the composition root) so it never lapses under a live token.
    version_ttl_secs: i64,
}

/// `INCR` then refresh `EXPIRE` atomically, so a failure between them can't strand a permanent key.
const INCR_WITH_TTL: &str = "local v = redis.call('INCR', KEYS[1])\n\
                             redis.call('EXPIRE', KEYS[1], ARGV[1])\n\
                             return v";

/// `GET` (missing key reads as 0) and refresh the TTL when the key exists.
const GET_WITH_TTL: &str = "local v = redis.call('GET', KEYS[1])\n\
                            if v then redis.call('EXPIRE', KEYS[1], ARGV[1]) return v end\n\
                            return 0";

impl RedisTokenRevocation {
    pub async fn new(url: &str, version_ttl_secs: u64) -> Result<Self, RepositoryError> {
        let client = Client::open(url).map_err(backend)?;
        let conn = ConnectionManager::new(client).await.map_err(backend)?;
        Ok(Self {
            conn,
            version_ttl_secs: i64::try_from(version_ttl_secs).unwrap_or(i64::MAX),
        })
    }
}

#[async_trait]
impl TokenRevocation for RedisTokenRevocation {
    async fn revoke(&self, jti: Uuid, ttl_secs: u64) -> Result<(), RepositoryError> {
        // Past-expiry tokens need no entry, and Redis rejects SET EX 0.
        if ttl_secs == 0 {
            return Ok(());
        }
        let mut conn = self.conn.clone();
        let _: () = conn
            .set_ex(denylist_key(jti), 1, ttl_secs)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn is_revoked(&self, jti: Uuid) -> Result<bool, RepositoryError> {
        let mut conn = self.conn.clone();
        conn.exists(denylist_key(jti)).await.map_err(backend)
    }

    async fn version(&self, user: UserId) -> Result<u64, RepositoryError> {
        let mut conn = self.conn.clone();
        Script::new(GET_WITH_TTL)
            .key(version_key(user))
            .arg(self.version_ttl_secs)
            .invoke_async(&mut conn)
            .await
            .map_err(backend)
    }

    async fn bump_version(&self, user: UserId) -> Result<u64, RepositoryError> {
        let mut conn = self.conn.clone();
        Script::new(INCR_WITH_TTL)
            .key(version_key(user))
            .arg(self.version_ttl_secs)
            .invoke_async(&mut conn)
            .await
            .map_err(backend)
    }
}

fn denylist_key(jti: Uuid) -> String {
    format!("portal:auth:denylist:{jti}")
}

fn version_key(user: UserId) -> String {
    format!("portal:auth:tokenver:{}", user.0)
}

fn backend<E: std::fmt::Display>(e: E) -> RepositoryError {
    RepositoryError::Backend(e.to_string())
}
