use async_trait::async_trait;

use crate::error::EventError;

/// Live payload feed for one topic; dropping it releases the subscription.
#[async_trait]
pub trait Subscription: Send {
    /// Next payload, or `None` once the backend closes the feed.
    async fn next(&mut self) -> Option<Vec<u8>>;
}

/// Consuming mirror of [`EventPublisher`](super::event_publisher::EventPublisher):
/// bytes out, callers deserialise their own wire format.
#[async_trait]
pub trait EventSubscriber: Send + Sync {
    async fn subscribe(&self, topic: &str) -> Result<Box<dyn Subscription>, EventError>;
}
