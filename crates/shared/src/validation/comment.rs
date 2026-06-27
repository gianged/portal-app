use crate::{
    errors::SharedError,
    validation::common::{self, COMMENT_BODY_MAX},
};

/// A comment must be non-empty and within [`COMMENT_BODY_MAX`].
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `body` is empty/whitespace-only or
/// longer than [`COMMENT_BODY_MAX`].
pub fn validate_comment_body(body: &str) -> Result<(), SharedError> {
    common::len_range("Comment", body, 1, COMMENT_BODY_MAX)
}
