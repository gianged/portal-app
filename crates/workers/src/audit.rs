use std::sync::Arc;

use apalis::prelude::{BoxDynError, Data, Error};

use application::{AuditProjector, DomainEvent};
use infrastructure::jobs::AuditEnvelope;

/// apalis task handler for the `audit` queue. Deserialises the queued domain
/// event and hands it to [`AuditProjector`], which appends the immutable audit
/// row.
///
/// A malformed payload or an append failure returns [`Error::Failed`] so apalis
/// applies its retry/backoff policy rather than silently dropping the job.
#[tracing::instrument(skip_all, err)]
pub async fn handle(
    envelope: AuditEnvelope,
    projector: Data<Arc<AuditProjector>>,
) -> Result<(), Error> {
    let event: DomainEvent = serde_json::from_slice(&envelope.event).map_err(failed)?;
    projector.handle(&event).await.map_err(failed)?;
    Ok(())
}

/// Wraps any concrete error into the apalis task error.
fn failed<E: std::error::Error + Send + Sync + 'static>(e: E) -> Error {
    Error::Failed(Arc::new(Box::new(e) as BoxDynError))
}
