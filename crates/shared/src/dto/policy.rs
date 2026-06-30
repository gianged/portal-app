use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Mirrors `domain::model::BalanceExpiryPolicy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BalanceExpiryPolicy {
    Warn,
    RecordWorkPct,
}

impl BalanceExpiryPolicy {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Warn => "Warn only",
            Self::RecordWorkPct => "Record work %",
        }
    }
}

/// The tunable attendance limits. Times are wire-encoded as `"HH:MM"`; hour
/// amounts are plain `f64`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDto {
    pub workday_start: String,
    pub work_hours_per_day: f64,
    pub flex_core_start: String,
    pub flex_core_end: String,
    pub flex_daily_min: f64,
    pub flex_daily_max: f64,
    pub flex_earliest_start: String,
    pub flex_latest_end: String,
    pub flex_max_segments: u16,
    pub flex_max_per_month: u16,
    pub overtime_max_hours_per_month: f64,
    pub balance_carry_years: u16,
    pub balance_expiry_policy: BalanceExpiryPolicy,
    pub balance_expiry_warn_days: u16,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// Full replacement of the policy. Maps to `application::commands::UpdatePolicyCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePolicyRequest {
    pub workday_start: String,
    pub work_hours_per_day: f64,
    pub flex_core_start: String,
    pub flex_core_end: String,
    pub flex_daily_min: f64,
    pub flex_daily_max: f64,
    pub flex_earliest_start: String,
    pub flex_latest_end: String,
    pub flex_max_segments: u16,
    pub flex_max_per_month: u16,
    pub overtime_max_hours_per_month: f64,
    pub balance_carry_years: u16,
    pub balance_expiry_policy: BalanceExpiryPolicy,
    pub balance_expiry_warn_days: u16,
}
