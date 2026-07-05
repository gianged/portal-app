use std::sync::Arc;

use apalis::prelude::{Data, Error};
use tracing::Span;

use application::{DomainEvent, NotificationFanout, resilience::CircuitBreaker};
use infrastructure::{jobs::NotificationEnvelope, telemetry};

use crate::job_error::{abort, failed, park_while_open, retryable};

/// apalis handler for the `notifications` queue: decodes the event and hands it to
/// [`NotificationFanout`]. Aborts on an undecodable payload (retrying can never fix
/// it); returns [`Error::Failed`] on fan-out errors so apalis retries.
///
/// Gated on the Postgres breaker: while it is circuit-open the job parks (see
/// [`park_while_open`]) so an outage costs at most one retry attempt per five
/// minutes instead of one per ~30s poll.
#[tracing::instrument(skip_all, err)]
pub async fn handle(
    envelope: NotificationEnvelope,
    fanout: Data<Arc<NotificationFanout>>,
    breaker: Data<Arc<CircuitBreaker>>,
) -> Result<(), Error> {
    if let Some(traceparent) = &envelope.traceparent {
        telemetry::set_parent_traceparent(&Span::current(), traceparent);
    }
    if !park_while_open(&breaker).await {
        return Err(retryable("postgres circuit open"));
    }
    let event: DomainEvent = serde_json::from_slice(&envelope.event).map_err(abort)?;
    tracing::debug!(topic = event.topic(), "notification job received");
    fanout.handle(&event).await.map_err(failed)?;
    Ok(())
}
