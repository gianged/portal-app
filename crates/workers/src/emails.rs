use std::sync::Arc;

use apalis::prelude::{BoxDynError, Data, Error};
use tracing::Span;

use domain::ports::mailer::{EmailMessage, Mailer};
use infrastructure::{jobs::EmailEnvelope, telemetry};

/// apalis handler for the `emails` queue: decodes the message and hands it to the
/// configured [`Mailer`]. Returns [`Error::Failed`] on bad payload or SMTP error so apalis retries.
#[tracing::instrument(skip_all, err)]
pub async fn handle(envelope: EmailEnvelope, mailer: Data<Arc<dyn Mailer>>) -> Result<(), Error> {
    if let Some(traceparent) = &envelope.traceparent {
        telemetry::set_parent_traceparent(&Span::current(), traceparent);
    }
    let message: EmailMessage = serde_json::from_slice(&envelope.message).map_err(failed)?;
    tracing::debug!(to = %message.to, "email job received");
    mailer.send(&message).await.map_err(failed)?;
    Ok(())
}

fn failed<E: std::error::Error + Send + Sync + 'static>(e: E) -> Error {
    Error::Failed(Arc::new(Box::new(e) as BoxDynError))
}
