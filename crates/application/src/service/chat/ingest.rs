use std::{sync::Arc, time::Duration};

use domain::{
    error::RepositoryError, ids::UserId, model::Message, ports::spool::Spool,
    repository::ChatRepository,
};
use tokio::{
    sync::{mpsc, oneshot},
    time::{self, MissedTickBehavior},
};

use super::ChatService;
use crate::{
    commands::chat::PostMessageCommand,
    error::{Error, Result},
    events::{DomainEvent, EventBus},
};

/// Tunables for the write-behind ingest buffer.
#[derive(Debug, Clone, Copy)]
pub struct ChatIngestConfig {
    /// Bounded channel depth; `enqueue` sheds load once this many messages are
    /// in flight.
    pub capacity: usize,
    /// Flush as soon as the buffer holds this many messages.
    pub batch_size: usize,
    /// Flush a partial buffer at least this often.
    pub flush_interval: Duration,
}

impl Default for ChatIngestConfig {
    fn default() -> Self {
        Self {
            capacity: 4096,
            batch_size: 256,
            flush_interval: Duration::from_millis(50),
        }
    }
}

/// Write-behind buffer in front of chat persistence. `enqueue` validates a post
/// and hands the built message to a bounded channel (optimistic ack); `run`
/// drains the channel, persists in batches, then fans out off the caller's task.
pub struct ChatIngest {
    chat: Arc<ChatService>,
    chats: Arc<dyn ChatRepository>,
    events: Arc<EventBus>,
    // Failed batches spill here (durable) and replay on recovery, honouring the optimistic ack.
    spool: Option<Arc<dyn Spool>>,
    tx: mpsc::Sender<Message>,
    cfg: ChatIngestConfig,
}

impl ChatIngest {
    /// Builds the buffer and returns it with the receiver half. The caller owns
    /// the `Receiver` and spawns [`ChatIngest::run`] with it. Pass a `spool` to
    /// make failed batches durable; `None` keeps the legacy drop-on-failure
    /// behaviour (used by tests with no Redis).
    #[must_use]
    pub fn new(
        chat: Arc<ChatService>,
        chats: Arc<dyn ChatRepository>,
        events: Arc<EventBus>,
        spool: Option<Arc<dyn Spool>>,
        cfg: ChatIngestConfig,
    ) -> (Arc<Self>, mpsc::Receiver<Message>) {
        let (tx, rx) = mpsc::channel(cfg.capacity);
        let ingest = Arc::new(Self {
            chat,
            chats,
            events,
            spool,
            tx,
            cfg,
        });
        (ingest, rx)
    }

    /// Validates the post and buffers it for batched persistence, returning the
    /// built message immediately (optimistic ack).
    ///
    /// # Errors
    /// Surfaces the same validation errors as `ChatService::post_message`, plus
    /// `Conflict("chat_overloaded")` when the buffer is full and
    /// `Conflict("chat_unavailable")` when the drain loop has stopped.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn enqueue(&self, actor: UserId, cmd: PostMessageCommand) -> Result<Message> {
        let message = self.chat.prepare_message(actor, cmd).await?;
        // Backpressure: shed load rather than await capacity, so a full buffer never stalls the WS task.
        match self.tx.try_send(message.clone()) {
            Ok(()) => Ok(message),
            Err(mpsc::error::TrySendError::Full(_)) => {
                Err(Error::Conflict("chat_overloaded".into()))
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(Error::Conflict("chat_unavailable".into()))
            }
        }
    }

    /// Drains the buffer, persisting batches and fanning out each message. Runs
    /// until every sender drops or `shutdown` fires, then flushes the tail. The
    /// explicit `shutdown` is needed because `self` and live WebSocket tasks hold
    /// senders, so they never all drop on their own.
    #[tracing::instrument(skip_all)]
    pub async fn run(
        self: Arc<Self>,
        mut rx: mpsc::Receiver<Message>,
        mut shutdown: oneshot::Receiver<()>,
    ) {
        let mut buf: Vec<Message> = Vec::with_capacity(self.cfg.batch_size);
        let mut ticker = time::interval(self.cfg.flush_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                received = rx.recv() => {
                    if let Some(message) = received {
                        buf.push(message);
                        if buf.len() >= self.cfg.batch_size {
                            self.flush(&mut buf).await;
                        }
                    } else {
                        // All senders dropped: drain the tail and exit.
                        self.flush(&mut buf).await;
                        break;
                    }
                }
                _ = ticker.tick() => self.flush(&mut buf).await,
                // Shutdown: stop accepting, sweep and flush the tail so optimistically-acked messages survive exit.
                _ = &mut shutdown => {
                    rx.close();
                    while let Ok(message) = rx.try_recv() {
                        buf.push(message);
                        if buf.len() >= self.cfg.batch_size {
                            self.flush(&mut buf).await;
                        }
                    }
                    self.flush(&mut buf).await;
                    break;
                }
            }
        }
    }

    /// Fans out a persisted batch off the WS task: a per-message broadcast pass then
    /// the batch's notification jobs. On a persist failure the batch is spilled to
    /// the durable spool (or dropped if none is configured), since optimistic ack
    /// already told senders their posts succeeded.
    async fn flush(&self, buf: &mut Vec<Message>) {
        if buf.is_empty() {
            return;
        }
        if let Err(e) = self.chats.save_messages(buf).await {
            self.spill(&e, buf).await;
            buf.clear();
            return;
        }

        let events: Vec<DomainEvent> = buf
            .drain(..)
            .map(|message| DomainEvent::MessagePosted {
                message_id: message.id,
                channel_id: message.channel_id,
                sender: message.sender_user_id,
                mentions: message.mentions.clone(),
                at: super::uuid_v7_created_at(message.id.0),
                after: message,
            })
            .collect();

        // Fan-out stays per-message and in publish order: Redis pub/sub preserves
        // order, which subscribers rely on within a channel.
        for event in &events {
            if let Err(e) = self.events.broadcast(event).await {
                tracing::warn!(error = %e, "chat ingest: broadcast failed");
            }
        }

        // Notifications enqueued after fan-out so a slow job queue never stalls broadcast ordering; only mentions notify.
        for event in &events {
            let mentioned = matches!(
                event,
                DomainEvent::MessagePosted { mentions, .. } if !mentions.is_empty()
            );
            if mentioned && let Err(e) = self.events.enqueue_notification(event).await {
                tracing::warn!(error = %e, "chat ingest: notification enqueue failed");
            }
        }
    }

    /// Spills a batch that failed to persist to the durable spool so the
    /// optimistic ack stays honoured; the drainer replays it once the backend
    /// recovers. With no spool configured the batch is dropped (legacy
    /// behaviour). Only the persist failure path reaches here.
    async fn spill(&self, cause: &RepositoryError, buf: &[Message]) {
        let Some(spool) = &self.spool else {
            tracing::error!(
                error = %cause,
                count = buf.len(),
                "chat ingest: batch persist failed, no spool configured, dropping"
            );
            return;
        };
        let bytes = match serde_json::to_vec(buf) {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::error!(
                    error = %cause,
                    serialize_error = %e,
                    count = buf.len(),
                    "chat ingest: batch persist failed, serialize for spool failed, dropping"
                );
                return;
            }
        };
        match spool.push(&bytes).await {
            Ok(()) => tracing::warn!(
                error = %cause,
                spilled = buf.len(),
                "chat ingest: batch persist failed, spilled to spool"
            ),
            Err(e) => tracing::error!(
                error = %cause,
                spool_error = %e,
                count = buf.len(),
                "chat ingest: batch persist failed AND spool push failed, dropping"
            ),
        }
    }
}
