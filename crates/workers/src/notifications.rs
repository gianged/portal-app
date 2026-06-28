use std::sync::Arc;

use apalis::prelude::{BoxDynError, Data, Error};
use tracing::Span;

use application::{DomainEvent, NotificationFanout, resilience::CircuitBreaker};
use domain::health::HealthStatus;
use infrastructure::{jobs::NotificationEnvelope, telemetry};

/// apalis handler for the `notifications` queue: decodes the event and hands it to
/// [`NotificationFanout`]. Returns [`Error::Failed`] on bad payload or fan-out error so apalis retries.
///
/// Gated on the Postgres breaker: while it is circuit-open the job is left queued
/// (retryable) so its retries are paced by the breaker cooldown instead of
/// hammering a dead backend and burning the apalis retry budget.
#[tracing::instrument(skip_all, err)]
pub async fn handle(
    envelope: NotificationEnvelope,
    fanout: Data<Arc<NotificationFanout>>,
    breaker: Data<Arc<CircuitBreaker>>,
) -> Result<(), Error> {
    if let Some(traceparent) = &envelope.traceparent {
        telemetry::set_parent_traceparent(&Span::current(), traceparent);
    }
    if breaker.status() == HealthStatus::Down {
        return Err(retryable("postgres circuit open"));
    }
    let event: DomainEvent = serde_json::from_slice(&envelope.event).map_err(failed)?;
    tracing::debug!(topic = event.topic(), "notification job received");
    fanout.handle(&event).await.map_err(failed)?;
    Ok(())
}

fn failed<E: std::error::Error + Send + Sync + 'static>(e: E) -> Error {
    Error::Failed(Arc::new(Box::new(e) as BoxDynError))
}

fn retryable(msg: &'static str) -> Error {
    Error::Failed(Arc::new(Box::<dyn std::error::Error + Send + Sync>::from(
        msg,
    )))
}
