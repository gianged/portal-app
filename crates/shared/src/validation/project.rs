use crate::{
    errors::SharedError,
    validation::common::{DESCRIPTION_MAX, NAME_MAX, NAME_MIN, len_range, max_len},
};

/// # Errors
///
/// Returns [`SharedError::Validation`] when `name` is empty or longer than
/// [`NAME_MAX`].
pub fn validate_project_name(name: &str) -> Result<(), SharedError> {
    len_range("Project name", name, NAME_MIN, NAME_MAX)
}

/// Description may be empty.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `description` exceeds
/// [`DESCRIPTION_MAX`].
pub fn validate_project_description(description: &str) -> Result<(), SharedError> {
    max_len("Project description", description, DESCRIPTION_MAX)
}

/// # Errors
///
/// Returns [`SharedError::Validation`] when `progress` exceeds 100.
pub fn validate_project_progress(progress: u8) -> Result<(), SharedError> {
    if progress <= 100 {
        Ok(())
    } else {
        Err(SharedError::Validation(
            "Progress must be between 0 and 100".to_owned(),
        ))
    }
}
