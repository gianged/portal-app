use apalis::prelude::Storage;
use apalis_redis::RedisStorage;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use domain::{error::JobError, ports::job_queue::JobQueue};

/// Wrapper persisted on the apalis/Redis queue. It carries the serialised
/// `application::DomainEvent` bytes verbatim; the worker deserialises them and
/// fans the event out into notifications. A struct (rather than a bare
/// `Vec<u8>`) leaves room for envelope metadata later without a wire-format
/// change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEnvelope {
    pub event: Vec<u8>,
}

/// Builds the Redis-backed job storage. Both the producer (`server`, via
/// [`ApalisNotificationQueue`]) and the consumer (`workers`) call this so they
/// agree on the queue's wire type and namespace.
pub async fn notification_storage(
    redis_url: &str,
) -> Result<RedisStorage<NotificationEnvelope>, JobError> {
    let conn = apalis_redis::connect(redis_url)
        .await
        .map_err(|e| JobError::Backend(e.to_string()))?;
    Ok(RedisStorage::new(conn))
}

/// [`JobQueue`] adapter over apalis + Redis. `RedisStorage` is cheap to clone
/// (it shares the underlying multiplexed connection), so `enqueue` clones per
/// call instead of locking — mirroring `RedisEventPublisher`.
#[derive(Clone)]
pub struct ApalisNotificationQueue {
    storage: RedisStorage<NotificationEnvelope>,
}

impl ApalisNotificationQueue {
    #[must_use]
    pub fn new(storage: RedisStorage<NotificationEnvelope>) -> Self {
        Self { storage }
    }
}

#[async_trait]
impl JobQueue for ApalisNotificationQueue {
    /// `queue` is accepted for the port's generality but ignored: this adapter
    /// is bound to a single notification storage at construction.
    async fn enqueue(&self, _queue: &str, payload: &[u8]) -> Result<(), JobError> {
        let mut storage = self.storage.clone();
        storage
            .push(NotificationEnvelope {
                event: payload.to_vec(),
            })
            .await
            .map_err(|e| JobError::Backend(e.to_string()))?;
        Ok(())
    }
}
