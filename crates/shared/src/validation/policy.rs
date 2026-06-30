use crate::{dto::policy::UpdatePolicyRequest, errors::SharedError, validation::common};

/// Parses a `"HH:MM"` clock time into `(hour, minute)`, or `None` if malformed.
#[must_use]
pub fn parse_hhmm(s: &str) -> Option<(u8, u8)> {
    let (h, m) = s.split_once(':')?;
    let hour: u8 = h.parse().ok()?;
    let minute: u8 = m.parse().ok()?;
    (hour < 24 && minute < 60).then_some((hour, minute))
}

/// Parses a `"HH:MM"` clock value into minutes-since-midnight.
pub(crate) fn minutes(s: &str, field: &str) -> Result<u16, SharedError> {
    let (h, m) = parse_hhmm(s)
        .ok_or_else(|| SharedError::Validation(format!("{field} must be a valid HH:MM time")))?;
    Ok(u16::from(h) * 60 + u16::from(m))
}

/// Validates a full policy update, mirroring the `attendance.policy` CHECKs.
///
/// # Errors
///
/// Returns [`SharedError::Validation`] when a time is malformed, a numeric bound
/// is out of range, or a window is out of order.
pub fn validate_policy(req: &UpdatePolicyRequest) -> Result<(), SharedError> {
    minutes(&req.workday_start, "Workday start")?;
    let core_start = minutes(&req.flex_core_start, "Flex core start")?;
    let core_end = minutes(&req.flex_core_end, "Flex core end")?;
    let earliest = minutes(&req.flex_earliest_start, "Flex earliest start")?;
    let latest = minutes(&req.flex_latest_end, "Flex latest end")?;

    common::in_range("Work hours per day", req.work_hours_per_day, 0.5, 24.0)?;
    common::in_range("Flex daily minimum", req.flex_daily_min, 0.0, 24.0)?;
    common::in_range("Flex daily maximum", req.flex_daily_max, 0.0, 24.0)?;
    common::in_range(
        "Overtime monthly cap",
        req.overtime_max_hours_per_month,
        0.5,
        744.0,
    )?;

    if req.flex_daily_min > req.flex_daily_max {
        return Err(SharedError::Validation(
            "Flex daily minimum must not exceed the maximum".into(),
        ));
    }
    if core_start >= core_end {
        return Err(SharedError::Validation(
            "Flex core start must be before the core end".into(),
        ));
    }
    if earliest > latest {
        return Err(SharedError::Validation(
            "Flex earliest start must not be after the latest end".into(),
        ));
    }
    if earliest > core_start || core_end > latest {
        return Err(SharedError::Validation(
            "Flex core window must sit inside the allowed envelope".into(),
        ));
    }
    if !(1..=4).contains(&req.flex_max_segments) {
        return Err(SharedError::Validation(
            "Flex maximum segments must be between 1 and 4".into(),
        ));
    }
    if req.balance_carry_years < 1 {
        return Err(SharedError::Validation(
            "Balance carry years must be at least 1".into(),
        ));
    }
    Ok(())
}
