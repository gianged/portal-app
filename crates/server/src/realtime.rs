//! Real-time plumbing for the chat WebSocket: a thin handle over the pub/sub ports.
//!
//! Two planes share one broker: durable `application::DomainEvent`s on `portal.*`
//! topics that the WS task projects to `ServerFrame`s, and ephemeral `WsSignal`s
//! (typing/presence/read-markers) on `portal.ws` that are best-effort, never persisted.
//!
//! Subscriptions are multiplexed through a process-wide hub: one backend
//! subscription per topic, fanned out to every WS connection over a broadcast
//! channel. Without the hub each connection held its own backend subscription
//! per topic, i.e. 4 Redis connections per connected user.

use std::{collections::HashMap, pin::Pin, sync::Arc, time::Duration};

use futures::{Stream, stream};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{
        Mutex,
        broadcast::{self, Receiver, Sender, error::RecvError},
    },
    time,
};

use domain::{
    error::EventError,
    ids::{ChannelId, MessageId, UserId},
    ports::{event_publisher::EventPublisher, event_subscriber::EventSubscriber},
};

/// Topic for ephemeral WS signals (not persisted, best-effort).
pub const WS_TOPIC: &str = "portal.ws";

/// Per-topic fan-out buffer; a consumer this far behind skips ahead (lagged)
/// instead of stalling the other connections.
const HUB_BUFFER: usize = 1024;

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
/// plus the topic hub used to open per-connection feeds.
#[derive(Clone)]
pub struct Realtime {
    publisher: Arc<dyn EventPublisher>,
    hub: Arc<Hub>,
}

impl Realtime {
    #[must_use]
    pub fn new(publisher: Arc<dyn EventPublisher>, subscriber: Arc<dyn EventSubscriber>) -> Self {
        Self {
            publisher,
            hub: Arc::new(Hub {
                subscriber,
                topics: Mutex::new(HashMap::new()),
            }),
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

    /// Subscribes to `topic` through the shared hub, yielding raw payload bytes.
    /// Never fails: the topic's pump (re)connects in the background, so during a
    /// broker outage the stream is silent instead of erroring.
    pub async fn subscribe(
        &self,
        topic: &str,
    ) -> Result<Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>, EventError> {
        let rx = self.hub.receiver(topic).await;
        Ok(Box::pin(stream::unfold(rx, |mut rx| async move {
            loop {
                match rx.recv().await {
                    Ok(payload) => return Some((payload, rx)),
                    // Slow consumer: skip ahead. Missed chat frames are
                    // recoverable via REST history; revocation is re-checked on
                    // the connection heartbeat.
                    Err(RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "realtime hub: consumer lagged");
                    }
                    Err(RecvError::Closed) => return None,
                }
            }
        })))
    }
}

/// One upstream subscription per topic, shared by every connection.
struct Hub {
    subscriber: Arc<dyn EventSubscriber>,
    topics: Mutex<HashMap<String, Sender<Vec<u8>>>>,
}

impl Hub {
    /// Returns a receiver for `topic`, spawning the topic's pump on first use.
    /// Pumps live for the process lifetime (the topic set is small and fixed).
    async fn receiver(&self, topic: &str) -> Receiver<Vec<u8>> {
        let mut topics = self.topics.lock().await;
        if let Some(tx) = topics.get(topic) {
            return tx.subscribe();
        }
        let (tx, rx) = broadcast::channel(HUB_BUFFER);
        topics.insert(topic.to_owned(), tx.clone());
        tokio::spawn(pump(self.subscriber.clone(), topic.to_owned(), tx));
        rx
    }
}

/// Forwards one topic's backend feed into its broadcast channel, reconnecting
/// with backoff. Send errors (no receivers) are ignored: an idle topic simply
/// drops payloads.
async fn pump(subscriber: Arc<dyn EventSubscriber>, topic: String, tx: Sender<Vec<u8>>) {
    let mut delay = Duration::from_millis(500);
    loop {
        match subscriber.subscribe(&topic).await {
            Ok(mut sub) => {
                while let Some(payload) = sub.next().await {
                    // Traffic proves the feed is healthy; reset the backoff.
                    delay = Duration::from_millis(500);
                    let _ = tx.send(payload);
                }
                tracing::warn!(topic = %topic, "realtime hub: subscription ended, reconnecting");
            }
            Err(e) => {
                tracing::warn!(topic = %topic, error = %e, "realtime hub: subscribe failed, retrying");
            }
        }
        time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(30));
    }
}
