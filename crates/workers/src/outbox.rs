//! Audit outbox projector loop: polls `audit.outbox_events` and projects each
//! record into the audit log. Exactly-once end to end: rows are written in the
//! entity transaction and the projection dedups on the record id.

use std::{sync::Arc, time::Duration};

use tokio::time;

use application::AuditProjector;

const BATCH: u32 = 64;
const IDLE: Duration = Duration::from_secs(2);

/// Drain loop. Runs forever; spawn under `supervise`. A drain error is logged
/// and retried next tick (unmarked records are simply re-claimed).
pub async fn run(projector: Arc<AuditProjector>) {
    loop {
        match projector.drain(BATCH).await {
            // A full batch means more is waiting; keep draining without idling.
            Ok(n) if n as u32 == BATCH => {}
            Ok(_) => time::sleep(IDLE).await,
            Err(e) => {
                tracing::warn!(error = %e, "audit outbox drain failed");
                time::sleep(IDLE).await;
            }
        }
    }
}
