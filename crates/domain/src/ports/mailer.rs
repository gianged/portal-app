use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::MailError;

/// One outbound email. Serializable because it crosses the durable job queue
/// between the fanout producer and the SMTP-sending worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub to: String,
    pub subject: String,
    /// Plain text — no HTML templating by design.
    pub body: String,
}

/// Outbound email transport (SMTP in production, log-only in dev).
#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, message: &EmailMessage) -> Result<(), MailError>;
}
