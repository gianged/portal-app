use async_trait::async_trait;

use crate::error::JobError;

/// Durable background-job sink. Bytes in, bytes out like [`EventPublisher`](super::event_publisher::EventPublisher);
/// `application` serialises the payload, the apalis-backed adapter persists it for a worker to consume.
#[async_trait]
pub trait JobQueue: Send + Sync {
    async fn enqueue(&self, queue: &str, payload: &[u8]) -> Result<(), JobError>;
}
