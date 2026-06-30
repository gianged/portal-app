use crate::{
    errors::SharedError,
    validation::common::{self, DESCRIPTION_MAX, NAME_MIN, TITLE_MAX},
};

/// # Errors
///
/// Returns [`SharedError::Validation`] when `title` is empty or longer than
/// [`TITLE_MAX`].
pub fn validate_request_title(title: &str) -> Result<(), SharedError> {
    common::len_range("Request title", title, NAME_MIN, TITLE_MAX)
}

/// Description may be empty.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `description` exceeds
/// [`DESCRIPTION_MAX`].
pub fn validate_request_description(description: &str) -> Result<(), SharedError> {
    common::max_len("Request description", description, DESCRIPTION_MAX)
}

/// # Errors
///
/// Returns [`SharedError::Validation`] when `progress` exceeds 100.
pub fn validate_request_progress(progress: u8) -> Result<(), SharedError> {
    if progress <= 100 {
        Ok(())
    } else {
        Err(SharedError::Validation(
            "Progress must be between 0 and 100".to_owned(),
        ))
    }
}
