use apalis::prelude::Storage;
use apalis_redis::RedisStorage;
use async_trait::async_trait;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use domain::{error::JobError, ports::job_queue::JobQueue};

use crate::telemetry;

/// Wire type persisted on one apalis/Redis queue: payload bytes plus the W3C
/// `traceparent` of the enqueuing request. Each implementor stays a distinct
/// type so its queue namespace (derived from the type name) cannot collide.
pub trait Envelope: Serialize + DeserializeOwned + Clone + Send + Sync + Unpin + 'static {
    fn new(payload: Vec<u8>, traceparent: Option<String>) -> Self;
}

/// Carries serialised `application::DomainEvent` bytes the worker fans out into
/// notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEnvelope {
    pub event: Vec<u8>,
    /// W3C `traceparent` of the enqueuing request, so the worker continues the
    /// same trace. Absent on jobs queued without active OTLP export.
    #[serde(default)]
    pub traceparent: Option<String>,
}

impl Envelope for NotificationEnvelope {
    fn new(payload: Vec<u8>, traceparent: Option<String>) -> Self {
        Self {
            event: payload,
            traceparent,
        }
    }
}

/// Audit-queue twin of [`NotificationEnvelope`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEnvelope {
    pub event: Vec<u8>,
    /// W3C `traceparent` of the enqueuing request; see [`NotificationEnvelope`].
    #[serde(default)]
    pub traceparent: Option<String>,
}

impl Envelope for AuditEnvelope {
    fn new(payload: Vec<u8>, traceparent: Option<String>) -> Self {
        Self {
            event: payload,
            traceparent,
        }
    }
}

/// Email-queue envelope, kept apart so SMTP retries never re-run the
/// non-idempotent notification fanout. Carries a serialised `EmailMessage`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailEnvelope {
    pub message: Vec<u8>,
    /// W3C `traceparent` of the enqueuing request; see [`NotificationEnvelope`].
    #[serde(default)]
    pub traceparent: Option<String>,
}

impl Envelope for EmailEnvelope {
    fn new(payload: Vec<u8>, traceparent: Option<String>) -> Self {
        Self {
            message: payload,
            traceparent,
        }
    }
}

/// Builds the Redis-backed job storage for `E`. Producer and consumer both call
/// this so they agree on wire type and namespace.
pub async fn storage<E: Envelope>(redis_url: &str) -> Result<RedisStorage<E>, JobError> {
    let conn = apalis_redis::connect(redis_url)
        .await
        .map_err(|e| JobError::Backend(e.to_string()))?;
    Ok(RedisStorage::new(conn))
}

/// Builds the Redis-backed notification job storage.
pub async fn notification_storage(
    redis_url: &str,
) -> Result<RedisStorage<NotificationEnvelope>, JobError> {
    storage::<NotificationEnvelope>(redis_url).await
}

/// Builds the Redis-backed audit job storage.
pub async fn audit_storage(redis_url: &str) -> Result<RedisStorage<AuditEnvelope>, JobError> {
    storage::<AuditEnvelope>(redis_url).await
}

/// Builds the Redis-backed email job storage.
pub async fn email_storage(redis_url: &str) -> Result<RedisStorage<EmailEnvelope>, JobError> {
    storage::<EmailEnvelope>(redis_url).await
}

/// [`JobQueue`] adapter over apalis + Redis, bound to one storage.
/// `RedisStorage` is cheap to clone, so `enqueue` clones per call instead of
/// locking.
#[derive(Clone)]
pub struct ApalisQueue<E: Envelope> {
    storage: RedisStorage<E>,
}

impl<E: Envelope> ApalisQueue<E> {
    #[must_use]
    pub fn new(storage: RedisStorage<E>) -> Self {
        Self { storage }
    }
}

#[async_trait]
impl<E: Envelope> JobQueue for ApalisQueue<E> {
    /// `queue` is ignored; the adapter is bound to one storage at construction.
    async fn enqueue(&self, _queue: &str, payload: &[u8]) -> Result<(), JobError> {
        let mut storage = self.storage.clone();
        storage
            .push(E::new(payload.to_vec(), telemetry::current_traceparent()))
            .await
            .map_err(|e| JobError::Backend(e.to_string()))?;
        Ok(())
    }
}

pub type ApalisNotificationQueue = ApalisQueue<NotificationEnvelope>;
pub type ApalisAuditQueue = ApalisQueue<AuditEnvelope>;
pub type ApalisEmailQueue = ApalisQueue<EmailEnvelope>;
