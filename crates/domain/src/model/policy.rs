use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{OffsetDateTime, Time};

use crate::ids::UserId;

/// What happens to a leave grant's unused days when it expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BalanceExpiryPolicy {
    /// Warn ahead of expiry, then silently lapse the days.
    Warn,
    /// Lapse the days and record the month's work percentage on the expiry txn.
    RecordWorkPct,
}

impl BalanceExpiryPolicy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::RecordWorkPct => "record_work_pct",
        }
    }
}

/// Raised when an [`AttendancePolicy`] fails its cross-field invariants.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid attendance policy: {0}")]
pub struct PolicyError(pub &'static str);

/// Tunable attendance limits, edited by HR / Director and cached at runtime. One
/// row exists in `attendance.policy`; the defaults here mirror that seed so the
/// system is usable before anyone edits it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttendancePolicy {
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
    pub updated_by_user_id: Option<UserId>,
    pub updated_at: OffsetDateTime,
}

impl AttendancePolicy {
    /// Checks the cross-field invariants enforced by the `attendance.policy` CHECKs.
    ///
    /// # Errors
    /// Returns [`PolicyError`] when a numeric bound is violated or a window is
    /// out of order.
    pub fn validate(&self) -> Result<(), PolicyError> {
        if self.work_hours_per_day <= 0.0 {
            return Err(PolicyError("work_hours_per_day must be positive"));
        }
        if self.flex_core_start >= self.flex_core_end {
            return Err(PolicyError("flex_core_start must be before flex_core_end"));
        }
        if self.flex_daily_min > self.flex_daily_max {
            return Err(PolicyError("flex_daily_min must not exceed flex_daily_max"));
        }
        if self.flex_earliest_start > self.flex_latest_end {
            return Err(PolicyError(
                "flex_earliest_start must not be after flex_latest_end",
            ));
        }
        if self.flex_earliest_start > self.flex_core_start
            || self.flex_core_end > self.flex_latest_end
        {
            return Err(PolicyError("flex core window must sit inside the envelope"));
        }
        if !(1..=4).contains(&self.flex_max_segments) {
            return Err(PolicyError("flex_max_segments must be between 1 and 4"));
        }
        if self.overtime_max_hours_per_month <= 0.0 {
            return Err(PolicyError("overtime_max_hours_per_month must be positive"));
        }
        if self.balance_carry_years < 1 {
            return Err(PolicyError("balance_carry_years must be at least 1"));
        }
        Ok(())
    }
}

impl Default for AttendancePolicy {
    fn default() -> Self {
        let at = |h, m| Time::from_hms(h, m, 0).expect("valid policy default time");
        Self {
            workday_start: at(8, 0),
            work_hours_per_day: 8.0,
            flex_core_start: at(10, 0),
            flex_core_end: at(15, 0),
            flex_daily_min: 4.0,
            flex_daily_max: 10.0,
            flex_earliest_start: at(8, 0),
            flex_latest_end: at(20, 0),
            flex_max_segments: 2,
            flex_max_per_month: 5,
            overtime_max_hours_per_month: 40.0,
            balance_carry_years: 3,
            balance_expiry_policy: BalanceExpiryPolicy::Warn,
            balance_expiry_warn_days: 60,
            updated_by_user_id: None,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        }
    }
}
