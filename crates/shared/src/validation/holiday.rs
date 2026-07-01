use crate::{
    dto::holiday::SetHolidayRequest,
    errors::SharedError,
    validation::common::{self, NAME_MAX, NAME_MIN},
};

/// Validates a holiday upsert: the name must be non-empty and within bounds.
///
/// # Errors
/// Returns [`SharedError::Validation`] when the name is empty or too long.
pub fn validate_holiday(req: &SetHolidayRequest) -> Result<(), SharedError> {
    common::len_range("Holiday name", &req.name, NAME_MIN, NAME_MAX)
}
