//! Domain <-> wire projections for the attendance policy.

use application::commands::policy::UpdatePolicyCommand;
use domain::model::{AttendancePolicy, BalanceExpiryPolicy};
use shared::{
    dto::policy::{BalanceExpiryPolicy as WireExpiry, PolicyDto, UpdatePolicyRequest},
    errors::SharedError,
    validation::policy::parse_hhmm,
};
use time::Time;

fn fmt_time(t: Time) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

fn to_time(s: &str, field: &str) -> Result<Time, SharedError> {
    let (h, m) = parse_hhmm(s)
        .ok_or_else(|| SharedError::Validation(format!("{field} must be a valid HH:MM time")))?;
    Time::from_hms(h, m, 0)
        .map_err(|_| SharedError::Validation(format!("{field} is not a valid time")))
}

#[must_use]
pub fn balance_expiry_policy_dto(v: BalanceExpiryPolicy) -> WireExpiry {
    match v {
        BalanceExpiryPolicy::Warn => WireExpiry::Warn,
        BalanceExpiryPolicy::RecordWorkPct => WireExpiry::RecordWorkPct,
    }
}

#[must_use]
pub fn balance_expiry_policy_domain(v: WireExpiry) -> BalanceExpiryPolicy {
    match v {
        WireExpiry::Warn => BalanceExpiryPolicy::Warn,
        WireExpiry::RecordWorkPct => BalanceExpiryPolicy::RecordWorkPct,
    }
}

#[must_use]
pub fn policy_dto(p: &AttendancePolicy) -> PolicyDto {
    PolicyDto {
        workday_start: fmt_time(p.workday_start),
        work_hours_per_day: p.work_hours_per_day,
        flex_core_start: fmt_time(p.flex_core_start),
        flex_core_end: fmt_time(p.flex_core_end),
        flex_daily_min: p.flex_daily_min,
        flex_daily_max: p.flex_daily_max,
        flex_earliest_start: fmt_time(p.flex_earliest_start),
        flex_latest_end: fmt_time(p.flex_latest_end),
        flex_max_segments: p.flex_max_segments,
        flex_max_per_month: p.flex_max_per_month,
        overtime_max_hours_per_month: p.overtime_max_hours_per_month,
        balance_carry_years: p.balance_carry_years,
        balance_expiry_policy: balance_expiry_policy_dto(p.balance_expiry_policy),
        balance_expiry_warn_days: p.balance_expiry_warn_days,
        updated_at: p.updated_at,
    }
}

/// # Errors
/// Returns [`SharedError::Validation`] when a `HH:MM` time field is malformed.
pub fn update_policy_command(req: UpdatePolicyRequest) -> Result<UpdatePolicyCommand, SharedError> {
    Ok(UpdatePolicyCommand {
        workday_start: to_time(&req.workday_start, "Workday start")?,
        work_hours_per_day: req.work_hours_per_day,
        flex_core_start: to_time(&req.flex_core_start, "Flex core start")?,
        flex_core_end: to_time(&req.flex_core_end, "Flex core end")?,
        flex_daily_min: req.flex_daily_min,
        flex_daily_max: req.flex_daily_max,
        flex_earliest_start: to_time(&req.flex_earliest_start, "Flex earliest start")?,
        flex_latest_end: to_time(&req.flex_latest_end, "Flex latest end")?,
        flex_max_segments: req.flex_max_segments,
        flex_max_per_month: req.flex_max_per_month,
        overtime_max_hours_per_month: req.overtime_max_hours_per_month,
        balance_carry_years: req.balance_carry_years,
        balance_expiry_policy: balance_expiry_policy_domain(req.balance_expiry_policy),
        balance_expiry_warn_days: req.balance_expiry_warn_days,
    })
}
