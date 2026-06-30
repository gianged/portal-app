use crate::{
    dto::overtime::{CreateOvertimeRequest, DecideOvertimeRequest},
    errors::SharedError,
    validation::common,
};

/// Generous per-request sanity bound on hours. This is NOT the legal cap: the
/// monthly limit lives in the attendance policy and is enforced server-side.
const HOURS_SANITY_MAX: f64 = 1_000.0;

/// Validates an overtime request: a positive `hours` amount and a bounded reason.
/// The monthly legal cap is enforced server-side from policy, not as a static
/// client bound.
///
/// # Errors
/// Returns [`SharedError::Validation`] when `hours` is not positive (or absurdly
/// large) or the reason is over-long.
pub fn validate_overtime(req: &CreateOvertimeRequest) -> Result<(), SharedError> {
    common::iso_date("Work date", &req.work_date)?;
    if req.hours <= 0.0 {
        return Err(SharedError::Validation(
            "Hours must be greater than 0".into(),
        ));
    }
    if req.hours > HOURS_SANITY_MAX {
        return Err(SharedError::Validation(format!(
            "Hours must be at most {HOURS_SANITY_MAX}"
        )));
    }
    common::max_len("Reason", &req.reason, common::DESCRIPTION_MAX)?;
    Ok(())
}

/// Validates an overtime decision: a bounded note.
///
/// # Errors
/// Returns [`SharedError::Validation`] when the note is too long.
pub fn validate_decide_overtime(req: &DecideOvertimeRequest) -> Result<(), SharedError> {
    common::max_len("Note", &req.note, common::DESCRIPTION_MAX)?;
    Ok(())
}
