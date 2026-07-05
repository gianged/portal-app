use std::sync::Arc;

use apalis::prelude::{Data, Error};
use tracing::Span;

use application::{AuditProjector, DomainEvent, resilience::CircuitBreaker};
use infrastructure::{jobs::AuditEnvelope, telemetry};

use crate::job_error::{abort, failed, park_while_open, retryable};

/// apalis handler for the `audit` queue: decodes the event and hands it to
/// [`AuditProjector`]. Aborts on an undecodable payload (retrying can never fix
/// it); returns [`Error::Failed`] on append errors so apalis retries.
///
/// Gated on the Postgres breaker: while it is circuit-open the job parks (see
/// [`park_while_open`]) so an outage costs at most one retry attempt per five
/// minutes instead of one per ~30s poll. Audit entries must survive outages.
#[tracing::instrument(skip_all, err)]
pub async fn handle(
    envelope: AuditEnvelope,
    projector: Data<Arc<AuditProjector>>,
    breaker: Data<Arc<CircuitBreaker>>,
) -> Result<(), Error> {
    if let Some(traceparent) = &envelope.traceparent {
        telemetry::set_parent_traceparent(&Span::current(), traceparent);
    }
    if !park_while_open(&breaker).await {
        return Err(retryable("postgres circuit open"));
    }
    let event: DomainEvent = serde_json::from_slice(&envelope.event).map_err(abort)?;
    projector.handle(&event).await.map_err(failed)?;
    Ok(())
}
