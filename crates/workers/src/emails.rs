use std::sync::Arc;

use apalis::prelude::{BoxDynError, Data, Error};

use domain::ports::mailer::{EmailMessage, Mailer};
use infrastructure::jobs::EmailEnvelope;

/// apalis task handler for the `emails` queue. Deserialises the queued message
/// and hands it to the configured [`Mailer`] (SMTP, or the dev log-only one).
///
/// A malformed payload or an SMTP failure returns [`Error::Failed`] so apalis
/// applies its retry/backoff policy rather than silently dropping the send.
#[tracing::instrument(skip_all, err)]
pub async fn handle(envelope: EmailEnvelope, mailer: Data<Arc<dyn Mailer>>) -> Result<(), Error> {
    let message: EmailMessage = serde_json::from_slice(&envelope.message).map_err(failed)?;
    tracing::debug!(to = %message.to, "email job received");
    mailer.send(&message).await.map_err(failed)?;
    Ok(())
}

/// Wraps any concrete error into the apalis task error.
fn failed<E: std::error::Error + Send + Sync + 'static>(e: E) -> Error {
    Error::Failed(Arc::new(Box::new(e) as BoxDynError))
}
