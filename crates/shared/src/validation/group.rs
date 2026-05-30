use crate::{
    errors::SharedError,
    validation::common::{DESCRIPTION_MAX, NAME_MAX, NAME_MIN, len_range, max_len},
};

/// # Errors
///
/// Returns [`SharedError::Validation`] when `name` is empty or longer than
/// [`NAME_MAX`].
pub fn validate_group_name(name: &str) -> Result<(), SharedError> {
    len_range("Group name", name, NAME_MIN, NAME_MAX)
}

/// Description may be empty.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when `description` exceeds
/// [`DESCRIPTION_MAX`].
pub fn validate_group_description(description: &str) -> Result<(), SharedError> {
    max_len("Group description", description, DESCRIPTION_MAX)
}
