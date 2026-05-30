use crate::{
    errors::SharedError,
    validation::common::{DESCRIPTION_MAX, NAME_MIN, TITLE_MAX, len_range, max_len},
};

/// # Errors
///
/// Returns [`SharedError::Validation`] when `title` is empty or longer than
/// [`TITLE_MAX`].
pub fn validate_request_title(title: &str) -> Result<(), SharedError> {
    len_range("Request title", title, NAME_MIN, TITLE_MAX)
}

/// Description may be empty.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `description` exceeds
/// [`DESCRIPTION_MAX`].
pub fn validate_request_description(description: &str) -> Result<(), SharedError> {
    max_len("Request description", description, DESCRIPTION_MAX)
}
