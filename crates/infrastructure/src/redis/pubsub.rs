use std::fmt::Display;
use std::pin::Pin;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use redis::{AsyncCommands, Client, aio::ConnectionManager};

use domain::{
    error::EventError,
    ports::{
        event_publisher::EventPublisher,
        event_subscriber::{EventSubscriber, Subscription},
    },
};

use crate::redis::connect_manager;

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
        let conn = connect_manager(url).await.map_err(backend)?;
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

/// Adapter implementing the [`EventSubscriber`] port over Redis pub/sub.
///
/// SUBSCRIBE takes over its connection, so each subscription opens a dedicated
/// one, owned by the returned handle and released when it drops.
pub struct RedisEventSubscriber {
    client: Client,
}

impl RedisEventSubscriber {
    /// Parses the URL eagerly; connections are opened per subscription.
    pub fn new(url: &str) -> Result<Self, EventError> {
        Ok(Self {
            client: Client::open(url).map_err(backend)?,
        })
    }
}

#[async_trait]
impl EventSubscriber for RedisEventSubscriber {
    async fn subscribe(&self, topic: &str) -> Result<Box<dyn Subscription>, EventError> {
        let mut pubsub = self.client.get_async_pubsub().await.map_err(backend)?;
        pubsub
            .subscribe(event_topic_key(topic))
            .await
            .map_err(backend)?;
        let stream = pubsub.into_on_message().filter_map(|msg| async move {
            match msg.get_payload::<Vec<u8>>() {
                Ok(payload) => Some(payload),
                Err(e) => {
                    tracing::warn!(error = %e, "dropping malformed pubsub payload");
                    None
                }
            }
        });
        Ok(Box::new(RedisSubscription {
            stream: Box::pin(stream),
        }))
    }
}

/// The boxed stream owns the pub/sub connection for the subscription's lifetime.
struct RedisSubscription {
    stream: Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>,
}

#[async_trait]
impl Subscription for RedisSubscription {
    async fn next(&mut self) -> Option<Vec<u8>> {
        self.stream.next().await
    }
}

fn backend<E: Display>(e: E) -> EventError {
    EventError::Backend(e.to_string())
}
