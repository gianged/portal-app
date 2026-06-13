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

/// Audit-queue twin of [`NotificationEnvelope`]. A distinct type so its
/// apalis/Redis namespace (`RedisStorage::new` namespaces by `type_name`) never
/// collides with the notification queue, keeping the two consumers' retry
/// domains isolated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEnvelope {
    pub event: Vec<u8>,
}

/// Builds the Redis-backed audit job storage. Both the producer (`server`, via
/// [`ApalisAuditQueue`]) and the consumer (`workers`) call this so they agree on
/// the queue's wire type and namespace.
pub async fn audit_storage(redis_url: &str) -> Result<RedisStorage<AuditEnvelope>, JobError> {
    let conn = apalis_redis::connect(redis_url)
        .await
        .map_err(|e| JobError::Backend(e.to_string()))?;
    Ok(RedisStorage::new(conn))
}

/// [`JobQueue`] adapter feeding the durable `audit` queue the audit projector
/// consumes. Mirrors [`ApalisNotificationQueue`].
#[derive(Clone)]
pub struct ApalisAuditQueue {
    storage: RedisStorage<AuditEnvelope>,
}

impl ApalisAuditQueue {
    #[must_use]
    pub fn new(storage: RedisStorage<AuditEnvelope>) -> Self {
        Self { storage }
    }
}

#[async_trait]
impl JobQueue for ApalisAuditQueue {
    /// `queue` is accepted for the port's generality but ignored: this adapter
    /// is bound to a single audit storage at construction.
    async fn enqueue(&self, _queue: &str, payload: &[u8]) -> Result<(), JobError> {
        let mut storage = self.storage.clone();
        storage
            .push(AuditEnvelope {
                event: payload.to_vec(),
            })
            .await
            .map_err(|e| JobError::Backend(e.to_string()))?;
        Ok(())
    }
}

/// Distinct type -> own apalis/Redis namespace, so SMTP retries never re-run the
/// non-idempotent notification fanout. Carries a serialised `EmailMessage`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailEnvelope {
    pub message: Vec<u8>,
}

/// Builds the Redis-backed email job storage. Producer (`ApalisEmailQueue`) and
/// consumer (`workers`) both call this to agree on wire type + namespace.
pub async fn email_storage(redis_url: &str) -> Result<RedisStorage<EmailEnvelope>, JobError> {
    let conn = apalis_redis::connect(redis_url)
        .await
        .map_err(|e| JobError::Backend(e.to_string()))?;
    Ok(RedisStorage::new(conn))
}

/// [`JobQueue`] adapter feeding the durable `emails` queue. Mirrors
/// [`ApalisNotificationQueue`].
#[derive(Clone)]
pub struct ApalisEmailQueue {
    storage: RedisStorage<EmailEnvelope>,
}

impl ApalisEmailQueue {
    #[must_use]
    pub fn new(storage: RedisStorage<EmailEnvelope>) -> Self {
        Self { storage }
    }
}

#[async_trait]
impl JobQueue for ApalisEmailQueue {
    /// `queue` ignored - this adapter is bound to one storage at construction.
    async fn enqueue(&self, _queue: &str, payload: &[u8]) -> Result<(), JobError> {
        let mut storage = self.storage.clone();
        storage
            .push(EmailEnvelope {
                message: payload.to_vec(),
            })
            .await
            .map_err(|e| JobError::Backend(e.to_string()))?;
        Ok(())
    }
}
