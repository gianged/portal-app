use crate::{
    dto::request::{CreateRequestRequest, SetRequestProgressRequest, UpdateRequestRequest},
    errors::SharedError,
    validation::{
        Validate,
        common::{self, DESCRIPTION_MAX, NAME_MIN, TITLE_MAX},
    },
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

impl Validate for CreateRequestRequest {
    fn validate(&self) -> Result<(), SharedError> {
        validate_request_title(&self.title)?;
        validate_request_description(&self.description)
    }
}

impl Validate for UpdateRequestRequest {
    fn validate(&self) -> Result<(), SharedError> {
        if let Some(title) = &self.title {
            validate_request_title(title)?;
        }
        if let Some(description) = &self.description {
            validate_request_description(description)?;
        }
        Ok(())
    }
}

impl Validate for SetRequestProgressRequest {
    fn validate(&self) -> Result<(), SharedError> {
        validate_request_progress(self.progress)
    }
}
