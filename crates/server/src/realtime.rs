//! Real-time plumbing for the chat WebSocket: a thin handle over the pub/sub ports.
//!
//! Two planes share one broker: durable `application::DomainEvent`s on `portal.*`
//! topics that the WS task projects to `ServerFrame`s, and ephemeral `WsSignal`s
//! (typing/presence/read-markers) on `portal.ws` that are best-effort, never persisted.

use std::{pin::Pin, sync::Arc};

use futures::{Stream, stream};
use serde::{Deserialize, Serialize};

use domain::{
    error::EventError,
    ids::{ChannelId, MessageId, UserId},
    ports::{event_publisher::EventPublisher, event_subscriber::EventSubscriber},
};

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

/// Shared handle stored in `AppState`: the publisher for the ephemeral plane
/// plus the subscriber used to open per-connection topic feeds.
#[derive(Clone)]
pub struct Realtime {
    publisher: Arc<dyn EventPublisher>,
    subscriber: Arc<dyn EventSubscriber>,
}

impl Realtime {
    #[must_use]
    pub fn new(publisher: Arc<dyn EventPublisher>, subscriber: Arc<dyn EventSubscriber>) -> Self {
        Self {
            publisher,
            subscriber,
        }
    }

    /// Publishes an ephemeral signal to the WS topic.
    ///
    /// # Panics
    ///
    /// Panics if `signal` fails to serialize, which cannot happen for `WsSignal`
    /// (all variants are plain serde-derivable types).
    pub async fn publish_signal(&self, signal: &WsSignal) -> Result<(), EventError> {
        let payload =
            serde_json::to_vec(signal).expect("WsSignal is composed of serde-derivable types");
        self.publisher.publish(WS_TOPIC, &payload).await
    }

    /// Subscribes to `topic`, yielding raw payload bytes. The returned stream
    /// owns its backend subscription for its lifetime.
    pub async fn subscribe(
        &self,
        topic: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>, EventError> {
        let sub = self.subscriber.subscribe(topic).await?;
        Ok(Box::pin(stream::unfold(sub, |mut sub| async move {
            sub.next().await.map(|payload| (payload, sub))
        })))
    }
}
