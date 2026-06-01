//! Real-time plumbing for the chat WebSocket: a thin handle over Redis pub/sub.
//!
//! Two planes share one Redis. (1) The durable `application::DomainEvent`s the
//! services emit to `portal.*` topics (chat, announcements) — the WS task
//! subscribes and projects them to `ServerFrame`s. (2) Ephemeral `WsSignal`s
//! (typing / presence / read-markers) the WS layer publishes itself to a
//! dedicated `portal.ws` topic; these are best-effort and never persisted.

use std::{pin::Pin, sync::Arc};

use futures::Stream;
use serde::{Deserialize, Serialize};

use domain::{
    error::EventError,
    ids::{ChannelId, MessageId, UserId},
    ports::event_publisher::EventPublisher,
};
use infrastructure::redis::subscribe;

/// Topic for ephemeral WS signals (not persisted, best-effort).
pub const WS_TOPIC: &str = "portal.ws";

/// Ephemeral real-time signals exchanged between connections. Distinct from
/// `application::DomainEvent` (which is durable and drives notifications).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsSignal {
    Typing {
        channel_id: ChannelId,
        user_id: UserId,
    },
    Presence {
        user_id: UserId,
        online: bool,
    },
    ReadMarker {
        channel_id: ChannelId,
        user_id: UserId,
        up_to: MessageId,
    },
}

/// Shared handle stored in `AppState`: the publisher connection for the
/// ephemeral plane plus the Redis URL used to open per-connection subscriptions.
#[derive(Clone)]
pub struct Realtime {
    publisher: Arc<dyn EventPublisher>,
    redis_url: Arc<str>,
}

impl Realtime {
    #[must_use]
    pub fn new(publisher: Arc<dyn EventPublisher>, redis_url: impl Into<Arc<str>>) -> Self {
        Self {
            publisher,
            redis_url: redis_url.into(),
        }
    }

    /// Publishes an ephemeral signal to the WS topic.
    pub async fn publish_signal(&self, signal: &WsSignal) -> Result<(), EventError> {
        let payload =
            serde_json::to_vec(signal).expect("WsSignal is composed of serde-derivable types");
        self.publisher.publish(WS_TOPIC, &payload).await
    }

    /// Opens a Redis subscription to `topic`, yielding raw payload bytes. The
    /// returned stream owns its own connection for its lifetime.
    pub async fn subscribe(
        &self,
        topic: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>, EventError> {
        subscribe(&self.redis_url, topic).await
    }
}
