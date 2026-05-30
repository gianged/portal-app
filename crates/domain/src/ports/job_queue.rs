use async_trait::async_trait;

use crate::error::JobError;

/// Durable background-job sink. Like [`EventPublisher`](super::event_publisher::EventPublisher),
/// it is bytes in, bytes out: the domain does not pick the wire format. Callers in
/// `application` serialise the job payload; the adapter persists it onto a queue
/// (`infrastructure` backs this with apalis) for a worker to consume later.
#[async_trait]
pub trait JobQueue: Send + Sync {
    async fn enqueue(&self, queue: &str, payload: &[u8]) -> Result<(), JobError>;
}
