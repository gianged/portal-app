use crate::{
    dto::project::{CreateProjectRequest, SetProjectProgressRequest, UpdateProjectMetadataRequest},
    errors::SharedError,
    validation::{
        Validate,
        common::{self, DESCRIPTION_MAX, NAME_MAX, NAME_MIN},
    },
};

/// # Errors
///
/// Returns [`SharedError::Validation`] when `name` is empty or longer than
/// [`NAME_MAX`].
pub fn validate_project_name(name: &str) -> Result<(), SharedError> {
    common::len_range("Project name", name, NAME_MIN, NAME_MAX)
}

/// Description may be empty.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `description` exceeds
/// [`DESCRIPTION_MAX`].
pub fn validate_project_description(description: &str) -> Result<(), SharedError> {
    common::max_len("Project description", description, DESCRIPTION_MAX)
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

impl Validate for CreateProjectRequest {
    fn validate(&self) -> Result<(), SharedError> {
        validate_project_name(&self.name)?;
        validate_project_description(&self.description)
    }
}

impl Validate for UpdateProjectMetadataRequest {
    fn validate(&self) -> Result<(), SharedError> {
        if let Some(name) = &self.name {
            validate_project_name(name)?;
        }
        if let Some(description) = &self.description {
            validate_project_description(description)?;
        }
        Ok(())
    }
}

impl Validate for SetProjectProgressRequest {
    fn validate(&self) -> Result<(), SharedError> {
        validate_project_progress(self.progress)
    }
}
