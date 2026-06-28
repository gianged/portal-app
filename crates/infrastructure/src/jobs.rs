use apalis::prelude::Storage;
use apalis_redis::RedisStorage;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use domain::{error::JobError, ports::job_queue::JobQueue};

use crate::telemetry;

/// Wrapper persisted on the apalis/Redis queue, carrying serialised
/// `application::DomainEvent` bytes the worker fans out into notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEnvelope {
    pub event: Vec<u8>,
    /// W3C `traceparent` of the enqueuing request, so the worker continues the
    /// same trace. Absent on jobs queued without active OTLP export.
    #[serde(default)]
    pub traceparent: Option<String>,
}

/// Builds the Redis-backed notification job storage, shared by producer and
/// consumer so they agree on wire type and namespace.
pub async fn notification_storage(
    redis_url: &str,
) -> Result<RedisStorage<NotificationEnvelope>, JobError> {
    let conn = apalis_redis::connect(redis_url)
        .await
        .map_err(|e| JobError::Backend(e.to_string()))?;
    Ok(RedisStorage::new(conn))
}

/// [`JobQueue`] adapter over apalis + Redis. `RedisStorage` is cheap to clone,
/// so `enqueue` clones per call instead of locking.
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
    /// `queue` is ignored; this adapter is bound to one notification storage.
    async fn enqueue(&self, _queue: &str, payload: &[u8]) -> Result<(), JobError> {
        let mut storage = self.storage.clone();
        storage
            .push(NotificationEnvelope {
                event: payload.to_vec(),
                traceparent: telemetry::current_traceparent(),
            })
            .await
            .map_err(|e| JobError::Backend(e.to_string()))?;
        Ok(())
    }
}

/// Audit-queue twin of [`NotificationEnvelope`]. A distinct type keeps its
/// apalis/Redis namespace from colliding with the notification queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEnvelope {
    pub event: Vec<u8>,
    /// W3C `traceparent` of the enqueuing request; see [`NotificationEnvelope`].
    #[serde(default)]
    pub traceparent: Option<String>,
}

/// Builds the Redis-backed audit job storage, shared by producer and consumer
/// so they agree on wire type and namespace.
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
    /// `queue` is ignored; this adapter is bound to one audit storage.
    async fn enqueue(&self, _queue: &str, payload: &[u8]) -> Result<(), JobError> {
        let mut storage = self.storage.clone();
        storage
            .push(AuditEnvelope {
                event: payload.to_vec(),
                traceparent: telemetry::current_traceparent(),
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
    /// W3C `traceparent` of the enqueuing request; see [`NotificationEnvelope`].
    #[serde(default)]
    pub traceparent: Option<String>,
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
                traceparent: telemetry::current_traceparent(),
            })
            .await
            .map_err(|e| JobError::Backend(e.to_string()))?;
        Ok(())
    }
}
