use crate::{
    dto::leave_balance::{AdjustBalanceRequest, SetLeaveGrantRequest},
    errors::SharedError,
    validation::{
        Validate,
        common::{self, DESCRIPTION_MAX},
    },
};

/// Sane upper bound on a single year's entitlement (days).
const DAYS_MAX: f64 = 366.0;

/// Validates an HR grant: year within `2000..=2100`, days non-negative, within
/// bounds, in half-day steps.
///
/// The upper bound stays well under the `time` crate's year ceiling (9999) so the
/// service-side expiry (`grant_year + carry_years`) never overflows the date range.
///
/// # Errors
/// Returns [`SharedError::Validation`] when `grant_year` is out of range, or
/// `days_granted` is negative, exceeds the cap, or is not a multiple of
/// `LEAVE_UNIT`.
pub fn validate_grant(req: &SetLeaveGrantRequest) -> Result<(), SharedError> {
    if !(2000..=2100).contains(&req.grant_year) {
        return Err(SharedError::Validation(
            "Grant year must be between 2000 and 2100".into(),
        ));
    }
    common::in_range("Days granted", req.days_granted, 0.0, DAYS_MAX)?;
    common::half_step("Days granted", req.days_granted)?;
    Ok(())
}

/// Validates a manual balance adjustment: half-day step, bounded reason.
///
/// # Errors
/// Returns [`SharedError::Validation`] when `delta` is not a multiple of
/// `LEAVE_UNIT` or the reason is too long.
pub fn validate_adjust(req: &AdjustBalanceRequest) -> Result<(), SharedError> {
    common::half_step("Adjustment", req.delta)?;
    common::max_len("Reason", &req.reason, DESCRIPTION_MAX)?;
    Ok(())
}

impl Validate for SetLeaveGrantRequest {
    fn validate(&self) -> Result<(), SharedError> {
        validate_grant(self)
    }
}

impl Validate for AdjustBalanceRequest {
    fn validate(&self) -> Result<(), SharedError> {
        validate_adjust(self)
    }
}
