use std::sync::Arc;

use apalis::prelude::{BoxDynError, Data, Error};

use application::{DomainEvent, NotificationFanout};
use infrastructure::jobs::NotificationEnvelope;

/// apalis handler for the `notifications` queue: decodes the event and hands it to
/// [`NotificationFanout`]. Returns [`Error::Failed`] on bad payload or fan-out error so apalis retries.
#[tracing::instrument(skip_all, err)]
pub async fn handle(
    envelope: NotificationEnvelope,
    fanout: Data<Arc<NotificationFanout>>,
) -> Result<(), Error> {
    let event: DomainEvent = serde_json::from_slice(&envelope.event).map_err(failed)?;
    tracing::debug!(topic = event.topic(), "notification job received");
    fanout.handle(&event).await.map_err(failed)?;
    Ok(())
}

fn failed<E: std::error::Error + Send + Sync + 'static>(e: E) -> Error {
    Error::Failed(Arc::new(Box::new(e) as BoxDynError))
}
