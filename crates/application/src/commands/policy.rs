use domain::model::BalanceExpiryPolicy;
use time::Time;

/// Full replacement of the tunable attendance limits. The editor loads the
/// current values, lets HR change any of them, and submits the complete set.
#[derive(Debug, Clone)]
pub struct UpdatePolicyCommand {
    pub workday_start: Time,
    pub work_hours_per_day: f64,
    pub flex_core_start: Time,
    pub flex_core_end: Time,
    pub flex_daily_min: f64,
    pub flex_daily_max: f64,
    pub flex_earliest_start: Time,
    pub flex_latest_end: Time,
    pub flex_max_segments: u16,
    pub flex_max_per_month: u16,
    pub overtime_max_hours_per_month: f64,
    pub balance_carry_years: u16,
    pub balance_expiry_policy: BalanceExpiryPolicy,
    pub balance_expiry_warn_days: u16,
}
