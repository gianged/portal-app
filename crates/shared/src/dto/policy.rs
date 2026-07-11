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
    /// Every variant, for building select options.
    pub const ALL: [Self; 2] = [Self::Warn, Self::RecordWorkPct];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Warn => "Warn only",
            Self::RecordWorkPct => "Record work %",
        }
    }

    /// Canonical wire string (the serde `snake_case` tag).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::RecordWorkPct => "record_work_pct",
        }
    }

    /// Parses a wire string produced by [`Self::as_str`].
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
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

#[cfg(test)]
mod tests {
    use super::BalanceExpiryPolicy;

    #[test]
    fn wire_helpers_match_serde() {
        for p in BalanceExpiryPolicy::ALL {
            assert_eq!(
                serde_json::to_string(&p).unwrap(),
                format!("\"{}\"", p.as_str())
            );
            assert_eq!(BalanceExpiryPolicy::from_wire(p.as_str()), Some(p));
        }
    }
}
