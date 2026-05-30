use std::sync::Arc;

use apalis::prelude::{BoxDynError, Data, Error};

use application::{DomainEvent, NotificationFanout};
use infrastructure::jobs::NotificationEnvelope;

/// apalis task handler for the `notifications` queue. Deserialises the queued
/// domain event and hands it to [`NotificationFanout`], which persists the
/// resulting notification rows.
///
/// A malformed payload or a fan-out failure returns [`Error::Failed`] so apalis
/// applies its retry/backoff policy rather than silently dropping the job.
pub async fn handle(
    envelope: NotificationEnvelope,
    fanout: Data<Arc<NotificationFanout>>,
) -> Result<(), Error> {
    let event: DomainEvent = serde_json::from_slice(&envelope.event).map_err(failed)?;
    fanout.handle(&event).await.map_err(failed)?;
    Ok(())
}

/// Wraps any concrete error into the apalis task error.
fn failed<E: std::error::Error + Send + Sync + 'static>(e: E) -> Error {
    Error::Failed(Arc::new(Box::new(e) as BoxDynError))
}
