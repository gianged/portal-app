use async_trait::async_trait;

use crate::error::EventError;

/// Bytes in, bytes out. Domain does not pick the wire format; callers in
/// `application` serialise their event payloads before publishing.
#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), EventError>;
}
