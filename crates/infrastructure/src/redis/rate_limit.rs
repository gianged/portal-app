use async_trait::async_trait;
use redis::{Client, Script, aio::ConnectionManager};
use time::OffsetDateTime;

use domain::{error::RepositoryError, ports::rate_limit::RateLimit};

/// Fixed-window rate limiter backed by `INCR` + `EXPIRE`.
///
/// Each call advances the counter for the current window (60 seconds wide by
/// default) and returns the post-increment value. The caller compares this
/// to the bucket-specific limit. The per-window key has a TTL twice the
/// window so late requests don't get a free counter reset; the key still
/// expires eventually (no infinite keys).
#[derive(Clone)]
pub struct RateLimiter {
    conn: ConnectionManager,
    window_secs: i64,
}

/// `INCR` then set `EXPIRE` on the first hit, atomically. Without this,
/// a transient `EXPIRE` failure between the two commands would leave the key
/// permanent, since later hits take the `count != 1` branch and never retry
/// the TTL.
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
    /// Increment the bucket's counter for the current window and return the
    /// resulting count. Caller decides whether the count exceeds the bucket's
    /// limit.
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

fn backend<E: std::fmt::Display>(e: E) -> RepositoryError {
    RepositoryError::Backend(e.to_string())
}
