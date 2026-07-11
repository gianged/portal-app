use crate::{
    dto::day_off::{CreateDayOffRequest, DecideDayOffRequest},
    errors::SharedError,
    validation::{
        Validate,
        common::{self, DESCRIPTION_MAX},
    },
};

/// Validates a leave request: `end >= start`, sensible half-day flags, and a
/// bounded reason. The past-date rule (allowed only for backdatable kinds) is
/// enforced server-side against the real clock.
///
/// # Errors
/// Returns [`SharedError::Validation`] on an end before the start, both half
/// flags on a single-day request, or an over-long reason.
pub fn validate_day_off(req: &CreateDayOffRequest) -> Result<(), SharedError> {
    if req.end_date < req.start_date {
        return Err(SharedError::Validation(
            "End date must not be before the start date".into(),
        ));
    }
    if req.start_date == req.end_date && req.start_half && req.end_half {
        return Err(SharedError::Validation(
            "Use a single half-day flag for a one-day request".into(),
        ));
    }
    common::max_len("Reason", &req.reason, DESCRIPTION_MAX)?;
    Ok(())
}

/// Validates a day-off decision: a bounded note.
///
/// # Errors
/// Returns [`SharedError::Validation`] when the note is too long.
pub fn validate_decide_day_off(req: &DecideDayOffRequest) -> Result<(), SharedError> {
    common::max_len("Note", &req.note, DESCRIPTION_MAX)?;
    Ok(())
}

impl Validate for CreateDayOffRequest {
    fn validate(&self) -> Result<(), SharedError> {
        validate_day_off(self)
    }
}

impl Validate for DecideDayOffRequest {
    fn validate(&self) -> Result<(), SharedError> {
        validate_decide_day_off(self)
    }
}
