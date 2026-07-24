//! Replays job dispatches the server spooled while both the gRPC and direct
//! apalis hops were unavailable, pushing each into the matching local apalis
//! storage. Same head-prefix drain/ack shape as the chat drainer.

use std::{sync::Arc, time::Duration};

use apalis::prelude::Storage;
use apalis_redis::RedisStorage;
use tokio::time;

use application::resilience::SpooledJob;
use domain::ports::{
    job_queue::{QUEUE_EMAILS, QUEUE_NOTIFICATIONS, QUEUE_REPAIR},
    spool::{Spool, SpoolId},
};
use infrastructure::jobs::{EmailEnvelope, Envelope, NotificationEnvelope, RepairEnvelope};

const BATCH: usize = 64;
const IDLE: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct JobSpoolDrainer {
    spool: Arc<dyn Spool>,
    notifications: RedisStorage<NotificationEnvelope>,
    emails: RedisStorage<EmailEnvelope>,
    repairs: RedisStorage<RepairEnvelope>,
}

impl JobSpoolDrainer {
    pub fn new(
        spool: Arc<dyn Spool>,
        notifications: RedisStorage<NotificationEnvelope>,
        emails: RedisStorage<EmailEnvelope>,
        repairs: RedisStorage<RepairEnvelope>,
    ) -> Self {
        Self {
            spool,
            notifications,
            emails,
            repairs,
        }
    }

    /// Drain-replay-ack loop. Runs forever; spawn under `supervise`.
    pub async fn run(self) {
        loop {
            let entries = match self.spool.peek(BATCH).await {
                Ok(entries) => entries,
                Err(e) => {
                    tracing::warn!(error = %e, "job spool drain failed");
                    time::sleep(IDLE).await;
                    continue;
                }
            };
            if entries.is_empty() {
                time::sleep(IDLE).await;
                continue;
            }

            // Replay the contiguous head prefix; stop at the first push failure
            // so the rest stays spooled and ordering is preserved.
            let mut acked: Vec<SpoolId> = Vec::new();
            let mut failed = false;
            for entry in &entries {
                match serde_json::from_slice::<SpooledJob>(&entry.payload) {
                    Ok(job) => {
                        if self.replay(job).await {
                            acked.push(entry.id);
                        } else {
                            failed = true;
                            break;
                        }
                    }
                    Err(e) => {
                        // Undecodable entries can never replay; drop them so they
                        // can't wedge the head of the backlog.
                        tracing::error!(error = %e, "dropping undecodable job spool entry");
                        acked.push(entry.id);
                    }
                }
            }

            let replayed = acked.len();
            if !acked.is_empty()
                && let Err(e) = self.spool.ack(&acked).await
            {
                tracing::warn!(error = %e, "job spool ack failed");
            }
            if replayed > 0 {
                tracing::info!(replayed, "replayed spooled job dispatches");
            }
            if failed || replayed < entries.len() {
                time::sleep(IDLE).await;
            }
        }
    }

    /// Pushes one spooled job into its queue's storage. `true` when the entry is
    /// settled (pushed, or dropped as poison), `false` on a retryable failure.
    async fn replay(&self, job: SpooledJob) -> bool {
        let result = match job.queue.as_str() {
            QUEUE_NOTIFICATIONS => {
                push(
                    self.notifications.clone(),
                    NotificationEnvelope::new(job.payload, job.traceparent),
                )
                .await
            }
            QUEUE_EMAILS => {
                push(
                    self.emails.clone(),
                    EmailEnvelope::new(job.payload, job.traceparent),
                )
                .await
            }
            QUEUE_REPAIR => {
                push(
                    self.repairs.clone(),
                    RepairEnvelope::new(job.payload, job.traceparent),
                )
                .await
            }
            other => {
                tracing::error!(queue = other, "dropping spooled job for unknown queue");
                return true;
            }
        };
        if let Err(e) = result {
            tracing::warn!(queue = %job.queue, error = %e, "job spool replay push failed");
            return false;
        }
        true
    }
}

/// Pushes one envelope into its apalis storage; callers map the native error
/// at their own boundary.
pub(crate) async fn push<E: Envelope>(
    mut storage: RedisStorage<E>,
    envelope: E,
) -> Result<(), <RedisStorage<E> as Storage>::Error> {
    storage.push(envelope).await.map(|_| ())
}
