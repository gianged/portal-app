use std::sync::Arc;

use apalis::prelude::{Data, Error};
use tracing::Span;

use application::{RepairJob, RepairService, resilience::CircuitBreaker};
use infrastructure::{jobs::RepairEnvelope, telemetry};

use crate::job_error::{abort, failed, park_while_open, retryable};

/// apalis handler for the `repair` queue: decodes the job and hands it to
/// [`RepairService`], whose reconciles re-derive the desired state from the DB
/// (idempotent). Aborts on an undecodable payload; returns [`Error::Failed`] on
/// reconcile errors so apalis redelivers.
///
/// Gated on the Postgres breaker: every reconcile starts from a DB read, so an
/// outage parks the job (see [`park_while_open`]) instead of burning retries.
#[tracing::instrument(skip_all, err)]
pub async fn handle(
    envelope: RepairEnvelope,
    service: Data<Arc<RepairService>>,
    breaker: Data<Arc<CircuitBreaker>>,
) -> Result<(), Error> {
    if let Some(traceparent) = &envelope.traceparent {
        telemetry::set_parent_traceparent(&Span::current(), traceparent);
    }
    if !park_while_open(&breaker).await {
        return Err(retryable("postgres circuit open"));
    }
    let job: RepairJob = serde_json::from_slice(&envelope.job).map_err(abort)?;
    tracing::info!(job = ?job, "repair job received");
    service.handle(job).await.map_err(failed)?;
    Ok(())
}
