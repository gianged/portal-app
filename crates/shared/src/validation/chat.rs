use crate::{
    errors::SharedError,
    validation::common::{MESSAGE_BODY_MAX, len_range},
};

/// A chat message must be non-empty and within [`MESSAGE_BODY_MAX`]. Attachments
/// and mentions are validated server-side against the channel.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `body` is empty/whitespace-only or
/// longer than [`MESSAGE_BODY_MAX`].
pub fn validate_message_body(body: &str) -> Result<(), SharedError> {
    len_range("Message", body, 1, MESSAGE_BODY_MAX)
}
