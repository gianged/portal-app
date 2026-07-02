use crate::{
    dto::chat::{EditMessageRequest, SendMessageRequest},
    errors::SharedError,
    validation::{
        Validate,
        common::{self, ATTACHMENT_KEYS_MAX, MENTIONS_MAX, MESSAGE_BODY_MAX, STORAGE_KEY_MAX},
    },
};

/// A chat message must be non-empty and within [`MESSAGE_BODY_MAX`]. Attachments
/// and mentions are validated server-side against the channel.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `body` is empty/whitespace-only or
/// longer than [`MESSAGE_BODY_MAX`].
pub fn validate_message_body(body: &str) -> Result<(), SharedError> {
    common::len_range("Message", body, 1, MESSAGE_BODY_MAX)
}

/// Caps the mention and attachment-key lists a message may carry; ownership of
/// the keys themselves is still verified server-side against the channel.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when either list exceeds its cap or an
/// attachment key exceeds [`STORAGE_KEY_MAX`].
pub fn validate_message_extras(
    mentions_len: usize,
    attachment_keys: &[String],
) -> Result<(), SharedError> {
    common::max_items("Mentions", mentions_len, MENTIONS_MAX)?;
    common::max_items("Attachments", attachment_keys.len(), ATTACHMENT_KEYS_MAX)?;
    for key in attachment_keys {
        common::max_len("Attachment key", key, STORAGE_KEY_MAX)?;
    }
    Ok(())
}

impl Validate for SendMessageRequest {
    fn validate(&self) -> Result<(), SharedError> {
        validate_message_body(&self.body)?;
        validate_message_extras(self.mentions.len(), &self.attachment_keys)
    }
}

impl Validate for EditMessageRequest {
    fn validate(&self) -> Result<(), SharedError> {
        validate_message_body(&self.body)
    }
}
