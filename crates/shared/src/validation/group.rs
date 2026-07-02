use crate::{
    dto::group::{CreateGroupRequest, UpdateGroupRequest},
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
pub fn validate_group_name(name: &str) -> Result<(), SharedError> {
    common::len_range("Group name", name, NAME_MIN, NAME_MAX)
}

/// Description may be empty.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `description` exceeds
/// [`DESCRIPTION_MAX`].
pub fn validate_group_description(description: &str) -> Result<(), SharedError> {
    common::max_len("Group description", description, DESCRIPTION_MAX)
}

impl Validate for CreateGroupRequest {
    fn validate(&self) -> Result<(), SharedError> {
        validate_group_name(&self.name)?;
        validate_group_description(&self.description)
    }
}

impl Validate for UpdateGroupRequest {
    fn validate(&self) -> Result<(), SharedError> {
        if let Some(name) = &self.name {
            validate_group_name(name)?;
        }
        if let Some(description) = &self.description {
            validate_group_description(description)?;
        }
        Ok(())
    }
}
