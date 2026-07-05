use std::sync::Arc;

use apalis::prelude::{Data, Error};
use tracing::Span;

use domain::ports::mailer::{EmailMessage, Mailer};
use infrastructure::{jobs::EmailEnvelope, telemetry};

use crate::job_error::{abort, failed};

/// apalis handler for the `emails` queue: decodes the message and hands it to the
/// configured [`Mailer`]. Aborts on an undecodable payload (retrying can never fix
/// it); returns [`Error::Failed`] on SMTP errors so apalis retries.
#[tracing::instrument(skip_all, err)]
pub async fn handle(envelope: EmailEnvelope, mailer: Data<Arc<dyn Mailer>>) -> Result<(), Error> {
    if let Some(traceparent) = &envelope.traceparent {
        telemetry::set_parent_traceparent(&Span::current(), traceparent);
    }
    let message: EmailMessage = serde_json::from_slice(&envelope.message).map_err(abort)?;
    tracing::debug!(to = %message.to, "email job received");
    mailer.send(&message).await.map_err(failed)?;
    Ok(())
}
