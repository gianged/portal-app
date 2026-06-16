//! Outbound email: a lettre/rustls SMTP transport and a log-only stand-in so dev needs no relay.

use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Attachment, Mailbox, MultiPart, SinglePart, header::ContentType},
    transport::smtp::authentication::Credentials,
};

use domain::{
    error::MailError,
    ports::mailer::{EmailMessage, Mailer},
};

/// SMTP transport security: StartTls (default) or None for plain internal relays on :25.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpTls {
    StartTls,
    None,
}

/// lettre-backed SMTP sender (rustls, async pool).
pub struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpMailer {
    pub fn new(
        host: &str,
        port: u16,
        username: Option<&str>,
        password: Option<&str>,
        from: &str,
        tls: SmtpTls,
    ) -> Result<Self, MailError> {
        let mut builder = match tls {
            SmtpTls::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
                .map_err(|e| MailError::Backend(e.to_string()))?,
            // Plain SMTP for in-network relays that terminate no TLS.
            SmtpTls::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host),
        }
        .port(port);
        if let (Some(user), Some(pass)) = (username, password) {
            builder = builder.credentials(Credentials::new(user.to_owned(), pass.to_owned()));
        }
        let from = from
            .parse::<Mailbox>()
            .map_err(|e| MailError::Invalid(format!("invalid SMTP_FROM address: {e}")))?;
        Ok(Self {
            transport: builder.build(),
            from,
        })
    }
}

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, message: &EmailMessage) -> Result<(), MailError> {
        let to = message
            .to
            .parse::<Mailbox>()
            .map_err(|e| MailError::Invalid(format!("invalid recipient address: {e}")))?;
        let builder = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(&message.subject);
        let email = if message.attachments.is_empty() {
            builder
                .body(message.body.clone())
                .map_err(|e| MailError::Invalid(e.to_string()))?
        } else {
            let mut multipart =
                MultiPart::mixed().singlepart(SinglePart::plain(message.body.clone()));
            for a in &message.attachments {
                let content_type = ContentType::parse(&a.content_type)
                    .map_err(|e| MailError::Invalid(e.to_string()))?;
                multipart = multipart.singlepart(
                    Attachment::new(a.filename.clone()).body(a.bytes.clone(), content_type),
                );
            }
            builder
                .multipart(multipart)
                .map_err(|e| MailError::Invalid(e.to_string()))?
        };
        self.transport
            .send(email)
            .await
            .map_err(|e| MailError::Backend(e.to_string()))?;
        Ok(())
    }
}

/// Dev stand-in: logs instead of sending (`EMAIL_ENABLED=false`).
pub struct LogMailer;

#[async_trait]
impl Mailer for LogMailer {
    async fn send(&self, message: &EmailMessage) -> Result<(), MailError> {
        tracing::info!(
            to = %message.to,
            subject = %message.subject,
            attachments = message.attachments.len(),
            "email suppressed (EMAIL_ENABLED=false): logging instead of sending"
        );
        Ok(())
    }
}
