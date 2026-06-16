use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::MailError;

/// A file attached to an outbound email. `bytes` stays raw (`domain` may not
/// add base64); it crosses the job queue serialized as a JSON byte array.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAttachment {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// One outbound email. Serializable because it crosses the durable job queue
/// between the fanout producer and the SMTP-sending worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub to: String,
    pub subject: String,
    /// Plain text — no HTML templating by design.
    pub body: String,
    /// Optional file attachments. Empty for notification emails; carries the PDF
    /// for the scheduled report mail. `#[serde(default)]` keeps older queued
    /// payloads (without this field) deserializable.
    #[serde(default)]
    pub attachments: Vec<EmailAttachment>,
}

/// Outbound email transport (SMTP in production, log-only in dev).
#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, message: &EmailMessage) -> Result<(), MailError>;
}
