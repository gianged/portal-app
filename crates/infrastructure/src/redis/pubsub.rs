use std::fmt::Display;
use std::pin::Pin;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use redis::{AsyncCommands, Client, aio::ConnectionManager};

use domain::{error::EventError, ports::event_publisher::EventPublisher};

/// Namespaced key for a domain-event topic, shared so publishers and subscribers stay aligned.
fn event_topic_key(topic: &str) -> String {
    format!("portal:event:{topic}")
}

/// Adapter implementing the [`EventPublisher`] port over Redis pub/sub.
///
/// `ConnectionManager` is `Arc`-shared and multiplexed, so cloning per call is cheap.
#[derive(Clone)]
pub struct RedisEventPublisher {
    conn: ConnectionManager,
}

impl RedisEventPublisher {
    pub async fn new(url: &str) -> Result<Self, EventError> {
        let client = Client::open(url).map_err(backend)?;
        let conn = ConnectionManager::new(client).await.map_err(backend)?;
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
            .map_err(backend)?;
        Ok(())
    }
}

/// Subscribe to a topic and return a stream of raw payload bytes.
///
/// Opens a fresh pub/sub connection held for the subscription's lifetime; the stream
/// owns it and releases the SUBSCRIBE when dropped.
pub async fn subscribe(
    url: &str,
    topic: &str,
) -> Result<Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>, EventError> {
    let client = Client::open(url).map_err(backend)?;
    let mut pubsub = client.get_async_pubsub().await.map_err(backend)?;
    let key = event_topic_key(topic);
    pubsub.subscribe(&key).await.map_err(backend)?;
    // Boxed so the stream is `'static`, owning its connection rather than borrowing url/topic.
    Ok(Box::pin(pubsub.into_on_message().filter_map(
        |msg| async move {
            match msg.get_payload::<Vec<u8>>() {
                Ok(payload) => Some(payload),
                Err(e) => {
                    tracing::warn!(error = %e, "dropping malformed pubsub payload");
                    None
                }
            }
        },
    )))
}

fn backend<E: Display>(e: E) -> EventError {
    EventError::Backend(e.to_string())
}
