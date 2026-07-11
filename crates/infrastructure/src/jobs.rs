use std::time::Duration;

use apalis::prelude::Storage;
use apalis_redis::{Config, ConnectionManager, RedisStorage};
use async_trait::async_trait;
use redis032::{Client, aio::ConnectionManagerConfig};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::time;

use domain::{error::JobError, ports::job_queue::JobQueue};

use crate::telemetry;

/// Wire type persisted on one apalis/Redis queue: payload bytes plus the W3C
/// `traceparent` of the enqueuing request. Each implementor stays a distinct
/// type with its own [`Envelope::NAMESPACE`] so queues cannot collide.
pub trait Envelope: Serialize + DeserializeOwned + Clone + Send + Sync + Unpin + 'static {
    /// Redis queue namespace. Frozen to the historical type-name-derived
    /// strings so queues persisted by earlier deploys keep draining.
    const NAMESPACE: &'static str;

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
    const NAMESPACE: &'static str = "infrastructure::jobs::NotificationEnvelope";

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
    const NAMESPACE: &'static str = "infrastructure::jobs::AuditEnvelope";

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
    const NAMESPACE: &'static str = "infrastructure::jobs::EmailEnvelope";

    fn new(payload: Vec<u8>, traceparent: Option<String>) -> Self {
        Self {
            message: payload,
            traceparent,
        }
    }
}

/// Builds the Redis-backed job storage for `E`. Producer and consumer share the
/// per-queue wrappers below so they agree on wire type and namespace. `buffer`
/// bounds how many jobs one consumer fetches and runs concurrently (producers
/// only push, so it is inert on their side).
async fn storage<E: Envelope>(redis_url: &str, buffer: usize) -> Result<RedisStorage<E>, JobError> {
    let conn = connect_apalis(redis_url).await?;
    let config = Config::default()
        .set_namespace(E::NAMESPACE)
        .set_buffer_size(buffer);
    Ok(RedisStorage::new_with_config(conn, config))
}

/// Builds the apalis connection with the same 2s connect/response timeouts as
/// every other Redis handle. apalis-redis pins its own `redis` major, so this
/// mirrors `redis::connect_manager` against that version instead of reusing it.
async fn connect_apalis(redis_url: &str) -> Result<ConnectionManager, JobError> {
    let client = Client::open(redis_url).map_err(|e| JobError::Backend(e.to_string()))?;
    let config = ConnectionManagerConfig::new()
        .set_connection_timeout(Duration::from_secs(2))
        .set_response_timeout(Duration::from_secs(2));
    ConnectionManager::new_with_config(client, config)
        .await
        .map_err(|e| JobError::Backend(e.to_string()))
}

/// Builds the Redis-backed notification job storage. The widest buffer: fanout
/// jobs are PG-bound and a company-wide announcement queues many of them.
pub async fn notification_storage(
    redis_url: &str,
) -> Result<RedisStorage<NotificationEnvelope>, JobError> {
    storage::<NotificationEnvelope>(redis_url, 32).await
}

/// Builds the Redis-backed audit job storage.
pub async fn audit_storage(redis_url: &str) -> Result<RedisStorage<AuditEnvelope>, JobError> {
    storage::<AuditEnvelope>(redis_url, 32).await
}

/// Builds the Redis-backed email job storage. Narrow: SMTP relays throttle.
pub async fn email_storage(redis_url: &str) -> Result<RedisStorage<EmailEnvelope>, JobError> {
    storage::<EmailEnvelope>(redis_url, 16).await
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
        let envelope = E::new(payload.to_vec(), telemetry::current_traceparent());
        let mut storage = self.storage.clone();
        match storage.push(envelope.clone()).await {
            Ok(_) => Ok(()),
            // One retry absorbs the lazy-reconnect first-command failure after a
            // Redis blip, so it never reaches the caller.
            Err(e) if is_connection_error(&e) => {
                time::sleep(Duration::from_millis(50)).await;
                storage
                    .push(envelope)
                    .await
                    .map(|_| ())
                    .map_err(|e| JobError::Backend(e.to_string()))
            }
            Err(e) => Err(JobError::Backend(e.to_string())),
        }
    }
}

fn is_connection_error(e: &apalis_redis::RedisError) -> bool {
    e.is_io_error() || e.is_connection_dropped() || e.is_connection_refusal() || e.is_timeout()
}

pub type ApalisNotificationQueue = ApalisQueue<NotificationEnvelope>;
pub type ApalisAuditQueue = ApalisQueue<AuditEnvelope>;
pub type ApalisEmailQueue = ApalisQueue<EmailEnvelope>;
