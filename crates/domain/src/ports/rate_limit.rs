use async_trait::async_trait;

use crate::error::RepositoryError;

/// Fixed-window request counter. Each [`RateLimit::incr`] advances the counter
/// for `bucket` in the current window and returns the post-increment value; the
/// caller compares it to the bucket's limit and rejects when exceeded. The
/// window width is an implementation detail of the adapter.
#[async_trait]
pub trait RateLimit: Send + Sync {
    /// Increment `bucket`'s counter for the current window and return the
    /// resulting count.
    async fn incr(&self, bucket: &str) -> Result<u64, RepositoryError>;
}
