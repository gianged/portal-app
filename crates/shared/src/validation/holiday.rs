use crate::{dto::holiday::SetHolidayRequest, errors::SharedError, validation::common};

/// Validates a holiday upsert: the name must be non-empty and within bounds.
///
/// # Errors
/// Returns [`SharedError::Validation`] when the name is empty or too long.
pub fn validate_holiday(req: &SetHolidayRequest) -> Result<(), SharedError> {
    common::len_range(
        "Holiday name",
        &req.name,
        common::NAME_MIN,
        common::NAME_MAX,
    )
}
