use async_trait::async_trait;
use futures::{Stream, StreamExt};
use redis::{AsyncCommands, Client, aio::ConnectionManager};

use domain::{error::EventError, ports::event_publisher::EventPublisher};

/// Namespaced key for a domain-event topic. Keeping the prefix here means
/// publishers and subscribers can't accidentally diverge.
fn event_topic_key(topic: &str) -> String {
    format!("portal:event:{topic}")
}

/// Adapter implementing the [`EventPublisher`] port over Redis pub/sub.
///
/// `ConnectionManager` is internally `Arc`-shared and multiplexed, so cloning
/// per call is cheap and lets concurrent publishes proceed in parallel without
/// an external lock.
#[derive(Clone)]
pub struct RedisEventPublisher {
    conn: ConnectionManager,
}

impl RedisEventPublisher {
    pub async fn new(url: &str) -> Result<Self, EventError> {
        let client = Client::open(url).map_err(|e| EventError::Backend(e.to_string()))?;
        let conn = ConnectionManager::new(client)
            .await
            .map_err(|e| EventError::Backend(e.to_string()))?;
        Ok(Self { conn })
    }
}

#[async_trait]
impl EventPublisher for RedisEventPublisher {
    async fn publish(&self, topic: &str, payload: &[u8]) -> Result<(), EventError> {
        let key = event_topic_key(topic);
        let mut conn = self.conn.clone();
        conn.publish::<_, _, ()>(key, payload)
            .await
            .map_err(|e| EventError::Backend(e.to_string()))?;
        Ok(())
    }
}

/// Subscribe to a topic and return a stream of raw payload bytes.
///
/// Pub/sub holds the connection for its lifetime, so this opens a fresh
/// `Client::get_async_pubsub` rather than reusing the publisher's
/// `ConnectionManager`. The returned stream is the consumer's responsibility
/// — when dropped, the SUBSCRIBE is implicitly released.
pub async fn subscribe(
    url: &str,
    topic: &str,
) -> Result<impl Stream<Item = Vec<u8>> + Send, EventError> {
    let client = Client::open(url).map_err(|e| EventError::Backend(e.to_string()))?;
    let mut pubsub = client
        .get_async_pubsub()
        .await
        .map_err(|e| EventError::Backend(e.to_string()))?;
    let key = event_topic_key(topic);
    pubsub
        .subscribe(&key)
        .await
        .map_err(|e| EventError::Backend(e.to_string()))?;
    Ok(pubsub
        .into_on_message()
        .filter_map(|msg| async move { msg.get_payload::<Vec<u8>>().ok() }))
}
