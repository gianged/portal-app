use crate::{
    errors::SharedError,
    validation::common::{self, MESSAGE_BODY_MAX},
};

/// An announcement body must be non-empty and within [`MESSAGE_BODY_MAX`].
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `body` is empty/whitespace-only or
/// longer than [`MESSAGE_BODY_MAX`].
pub fn validate_announcement_body(body: &str) -> Result<(), SharedError> {
    common::len_range("Announcement", body, 1, MESSAGE_BODY_MAX)
}
