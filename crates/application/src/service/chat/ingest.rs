use std::sync::Arc;
use std::time::Duration;

use domain::{ids::UserId, model::Message, repository::ChatRepository};
use tokio::sync::{mpsc, oneshot};
use tokio::time::MissedTickBehavior;

use crate::{
    commands::chat::PostMessageCommand,
    error::{Error, Result},
    events::{DomainEvent, EventBus},
};

use super::{ChatService, uuid_v7_created_at};

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
    tx: mpsc::Sender<Message>,
    cfg: ChatIngestConfig,
}

impl ChatIngest {
    /// Builds the buffer and returns it with the receiver half. The caller owns
    /// the `Receiver` and spawns [`ChatIngest::run`] with it.
    #[must_use]
    pub fn new(
        chat: Arc<ChatService>,
        chats: Arc<dyn ChatRepository>,
        events: Arc<EventBus>,
        cfg: ChatIngestConfig,
    ) -> (Arc<Self>, mpsc::Receiver<Message>) {
        let (tx, rx) = mpsc::channel(cfg.capacity);
        let ingest = Arc::new(Self {
            chat,
            chats,
            events,
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
    pub async fn enqueue(&self, actor: UserId, cmd: PostMessageCommand) -> Result<Message> {
        let message = self.chat.prepare_message(actor, cmd).await?;
        // Backpressure policy: shed load rather than await capacity. A full buffer
        // means the drain can't keep up, so blocking here would only stall the WS
        // task and back-pressure into the whole connection; instead surface
        // `chat_overloaded` and let the client retry.
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
    /// until either every sender is dropped or `shutdown` fires, then flushes the
    /// tail and returns. The explicit `shutdown` is what makes graceful drain work
    /// at process exit: the loop holds a `tx` via `self` and live WebSocket tasks
    /// hold more, so the senders never all drop on their own.
    pub async fn run(
        self: Arc<Self>,
        mut rx: mpsc::Receiver<Message>,
        mut shutdown: oneshot::Receiver<()>,
    ) {
        let mut buf: Vec<Message> = Vec::with_capacity(self.cfg.batch_size);
        let mut ticker = tokio::time::interval(self.cfg.flush_interval);
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
                // Shutdown signalled (or its sender dropped): stop accepting, sweep
                // whatever is still queued, and flush the tail so optimistically
                // -acked messages survive exit. A message a live client sends after
                // this lands on a closed channel and is rejected, bounded by the
                // server's force-exit watchdog.
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

    /// Fans out a persisted batch off the WS task: a per-message broadcast pass
    /// (the real-time fan-out) followed by the batch's notification jobs enqueued
    /// together. On a persist failure the batch is logged and dropped: under
    /// optimistic ack the senders were already told their posts succeeded.
    async fn flush(&self, buf: &mut Vec<Message>) {
        if buf.is_empty() {
            return;
        }
        if let Err(e) = self.chats.save_messages(buf).await {
            tracing::error!(
                error = %e,
                count = buf.len(),
                "chat ingest: batch persist failed, dropping"
            );
            buf.clear();
            return;
        }

        // Build one event per persisted message, draining the buffer.
        let events: Vec<DomainEvent> = buf
            .drain(..)
            .map(|message| DomainEvent::MessagePosted {
                message_id: message.id,
                channel_id: message.channel_id,
                sender: message.sender_user_id,
                mentions: message.mentions.clone(),
                at: uuid_v7_created_at(message.id.0),
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

        // Side channel, enqueued together after the fan-out so a slow job queue
        // never stalls broadcast ordering. Only mention-bearing messages notify;
        // chat is Scylla-backed and absent from AUDIT_TOPICS, so there are no
        // audit jobs to enqueue here.
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
}
