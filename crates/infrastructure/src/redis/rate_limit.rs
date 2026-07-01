use std::fmt::Display;

use async_trait::async_trait;
use redis::{Client, Script, aio::ConnectionManager};
use time::OffsetDateTime;

use domain::{error::RepositoryError, ports::rate_limit::RateLimit};

/// Fixed-window rate limiter backed by `INCR` + `EXPIRE`.
///
/// Each call advances the current window's counter and returns it; the key's TTL is
/// twice the window so late requests can't reset the count early.
#[derive(Clone)]
pub struct RateLimiter {
    conn: ConnectionManager,
    window_secs: i64,
}

/// `INCR` then `EXPIRE` on the first hit, atomically, so a failed `EXPIRE` can't strand a permanent key.
const INCR_WITH_TTL: &str = "local v = redis.call('INCR', KEYS[1])\n\
                             if v == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end\n\
                             return v";

impl RateLimiter {
    pub async fn new(url: &str) -> Result<Self, RepositoryError> {
        let client = Client::open(url).map_err(backend)?;
        let conn = ConnectionManager::new(client).await.map_err(backend)?;
        Ok(Self {
            conn,
            window_secs: 60,
        })
    }

    #[must_use]
    pub const fn with_window(mut self, window_secs: i64) -> Self {
        self.window_secs = window_secs;
        self
    }
}

#[async_trait]
impl RateLimit for RateLimiter {
    #[tracing::instrument(skip_all)]
    async fn incr(&self, bucket: &str) -> Result<u64, RepositoryError> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let window = now / self.window_secs;
        let key = rate_limit_key(bucket, window);
        let ttl = self.window_secs.saturating_mul(2);

        let mut conn = self.conn.clone();
        let count: u64 = Script::new(INCR_WITH_TTL)
            .key(&key)
            .arg(ttl)
            .invoke_async(&mut conn)
            .await
            .map_err(backend)?;
        Ok(count)
    }
}

fn rate_limit_key(bucket: &str, window: i64) -> String {
    format!("portal:ratelimit:{bucket}:{window}")
}

fn backend<E: Display>(e: E) -> RepositoryError {
    RepositoryError::Backend(e.to_string())
}
