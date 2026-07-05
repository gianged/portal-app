use std::{sync::Arc, time::Duration};

use tokio::time;

use domain::{
    health::HealthStatus,
    model::Message,
    ports::spool::{Spool, SpoolId},
    repository::ChatRepository,
};

use super::{backoff::Backoff, circuit::CircuitBreaker};

/// Tunables for the replay drainer.
#[derive(Debug, Clone, Copy)]
pub struct DrainerConfig {
    /// Entries replayed per cycle while the backend is recovering (half-open).
    pub probe_batch: usize,
    /// Entries replayed per cycle once the backend is healthy.
    pub max_batch: usize,
    /// Sleep when the backlog is empty, the backend is down, or a replay failed.
    pub idle_interval: Duration,
}

impl Default for DrainerConfig {
    fn default() -> Self {
        Self {
            probe_batch: 16,
            max_batch: 256,
            idle_interval: Duration::from_millis(500),
        }
    }
}

/// Replays the chat spool back into the chat store once the backend recovers,
/// without flooding it. Gates on the backend's circuit breaker: while `Down` it
/// waits, while `Degraded` (recovering) it replays a small probe batch, and only
/// once `Up` does it replay full batches. Successful replays feed the breaker
/// toward closed; a replay failure feeds it back toward open.
///
/// Cheap to clone (all fields are `Arc` or `Copy`), so a supervisor can rebuild
/// the run future on restart.
#[derive(Clone)]
pub struct Drainer {
    spool: Arc<dyn Spool>,
    chats: Arc<dyn ChatRepository>,
    breaker: Arc<CircuitBreaker>,
    cfg: DrainerConfig,
}

impl Drainer {
    #[must_use]
    pub fn new(
        spool: Arc<dyn Spool>,
        chats: Arc<dyn ChatRepository>,
        breaker: Arc<CircuitBreaker>,
        cfg: DrainerConfig,
    ) -> Self {
        Self {
            spool,
            chats,
            breaker,
            cfg,
        }
    }

    /// Drain-replay-ack loop. Runs forever; spawn under `supervise`.
    pub async fn run(self) {
        // Jittered inter-cycle pause used only while a full backlog remains, so a
        // large spool replays as a paced stream rather than a single flood.
        let mut pace = Backoff::new(Duration::from_millis(5), Duration::from_millis(100));
        loop {
            let batch = match self.breaker.status() {
                HealthStatus::Down => {
                    time::sleep(self.cfg.idle_interval).await;
                    continue;
                }
                HealthStatus::Degraded => self.cfg.probe_batch,
                HealthStatus::Up => self.cfg.max_batch,
            };

            let entries = match self.spool.drain(batch).await {
                Ok(entries) => entries,
                Err(e) => {
                    tracing::warn!(error = %e, "chat drainer: spool drain failed");
                    time::sleep(self.cfg.idle_interval).await;
                    continue;
                }
            };
            if entries.is_empty() {
                time::sleep(self.cfg.idle_interval).await;
                continue;
            }

            // Replay the contiguous head prefix; stop at the first replay failure
            // so the rest stays spooled and ordering is preserved.
            let mut acked: Vec<SpoolId> = Vec::new();
            let mut failed = false;
            for entry in &entries {
                match serde_json::from_slice::<Vec<Message>>(&entry.payload) {
                    Ok(messages) => match self.chats.save_messages(&messages).await {
                        Ok(()) => {
                            self.breaker.record_success();
                            acked.push(entry.id);
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "chat drainer: replay persist failed");
                            self.breaker.record_failure();
                            failed = true;
                            break;
                        }
                    },
                    Err(e) => {
                        // Undecodable entry can never replay; drop it so it can't
                        // wedge the head of the backlog forever.
                        tracing::error!(error = %e, "chat drainer: dropping undecodable spool entry");
                        acked.push(entry.id);
                    }
                }
            }

            let replayed = acked.len();
            if !acked.is_empty()
                && let Err(e) = self.spool.ack(&acked).await
            {
                tracing::warn!(error = %e, "chat drainer: spool ack failed");
            }
            if replayed > 0 {
                tracing::info!(replayed, "chat drainer: replayed spooled batches");
            }

            if failed || replayed < batch {
                pace.reset();
                time::sleep(self.cfg.idle_interval).await;
            } else {
                // Full batch drained: more likely remains, pace the next cycle.
                time::sleep(pace.next_delay()).await;
            }
        }
    }
}
