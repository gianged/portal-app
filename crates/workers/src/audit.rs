use std::sync::Arc;

use apalis::prelude::{BoxDynError, Data, Error};

use application::{AuditProjector, DomainEvent};
use infrastructure::jobs::AuditEnvelope;

/// apalis handler for the `audit` queue: decodes the event and hands it to
/// [`AuditProjector`]. Returns [`Error::Failed`] on bad payload or append error so apalis retries.
#[tracing::instrument(skip_all, err)]
pub async fn handle(
    envelope: AuditEnvelope,
    projector: Data<Arc<AuditProjector>>,
) -> Result<(), Error> {
    let event: DomainEvent = serde_json::from_slice(&envelope.event).map_err(failed)?;
    projector.handle(&event).await.map_err(failed)?;
    Ok(())
}

fn failed<E: std::error::Error + Send + Sync + 'static>(e: E) -> Error {
    Error::Failed(Arc::new(Box::new(e) as BoxDynError))
}
