use std::sync::Arc;

use apalis::prelude::{BoxDynError, Data, Error};

use application::{AuditProjector, DomainEvent, resilience::CircuitBreaker};
use domain::health::HealthStatus;
use infrastructure::jobs::AuditEnvelope;

/// apalis handler for the `audit` queue: decodes the event and hands it to
/// [`AuditProjector`]. Returns [`Error::Failed`] on bad payload or append error so apalis retries.
///
/// Gated on the Postgres breaker: while it is circuit-open the job is left queued
/// (retryable) so its retries are paced by the breaker cooldown.
#[tracing::instrument(skip_all, err)]
pub async fn handle(
    envelope: AuditEnvelope,
    projector: Data<Arc<AuditProjector>>,
    breaker: Data<Arc<CircuitBreaker>>,
) -> Result<(), Error> {
    if breaker.status() == HealthStatus::Down {
        return Err(retryable("postgres circuit open"));
    }
    let event: DomainEvent = serde_json::from_slice(&envelope.event).map_err(failed)?;
    projector.handle(&event).await.map_err(failed)?;
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
