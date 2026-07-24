use async_trait::async_trait;

use crate::error::JobError;

/// Queue names shared by the dispatch hops and the workers-side consumers.
pub const QUEUE_NOTIFICATIONS: &str = "notifications";
pub const QUEUE_EMAILS: &str = "emails";
pub const QUEUE_REPAIR: &str = "repair";

/// Durable background-job sink. Bytes in, bytes out like [`EventPublisher`](super::event_publisher::EventPublisher);
/// `application` serialises the payload, the apalis-backed adapter persists it for a worker to consume.
#[async_trait]
pub trait JobQueue: Send + Sync {
    async fn enqueue(&self, queue: &str, payload: &[u8]) -> Result<(), JobError>;
}
