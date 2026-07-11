use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::dto::{common::UserSummaryDto, ids::DayOffId};

/// Mirrors `domain::model::DayOffKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayOffKind {
    AnnualLeave,
    SickLeave,
    UnpaidLeave,
    Remote,
    Other,
}

impl DayOffKind {
    /// Every variant, for building select options.
    pub const ALL: [Self; 5] = [
        Self::AnnualLeave,
        Self::SickLeave,
        Self::UnpaidLeave,
        Self::Remote,
        Self::Other,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::AnnualLeave => "Annual leave",
            Self::SickLeave => "Sick leave",
            Self::UnpaidLeave => "Unpaid leave",
            Self::Remote => "Remote",
            Self::Other => "Other",
        }
    }

    /// Canonical wire string (the serde `snake_case` tag).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AnnualLeave => "annual_leave",
            Self::SickLeave => "sick_leave",
            Self::UnpaidLeave => "unpaid_leave",
            Self::Remote => "remote",
            Self::Other => "other",
        }
    }

    /// Parses a wire string produced by [`Self::as_str`].
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }

    /// Annual leave needs leader then HR; everything else is leader-only.
    #[must_use]
    pub fn requires_hr_approval(self) -> bool {
        matches!(self, Self::AnnualLeave)
    }

    /// Sick / unpaid leave may be filed for a past date.
    #[must_use]
    pub fn allows_backdate(self) -> bool {
        matches!(self, Self::SickLeave | Self::UnpaidLeave)
    }

    /// Only annual leave draws down the balance.
    #[must_use]
    pub fn consumes_balance(self) -> bool {
        matches!(self, Self::AnnualLeave)
    }
}

/// Mirrors `domain::model::DayOffStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayOffStatus {
    Pending,
    LeaderApproved,
    Approved,
    Rejected,
    Cancelled,
}

impl DayOffStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::LeaderApproved => "Leader approved",
            Self::Approved => "Approved",
            Self::Rejected => "Rejected",
            Self::Cancelled => "Cancelled",
        }
    }
}

/// A leave request with its requester and decision metadata. Dates are
/// `"YYYY-MM-DD"`; `days` is the computed working-day count.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayOffDto {
    pub id: DayOffId,
    pub requester: UserSummaryDto,
    pub kind: DayOffKind,
    pub start_date: Date,
    pub end_date: Date,
    pub start_half: bool,
    pub end_half: bool,
    pub days: f64,
    pub reason: String,
    pub status: DayOffStatus,
    pub leader: Option<UserSummaryDto>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub leader_decided_at: Option<OffsetDateTime>,
    pub hr: Option<UserSummaryDto>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub hr_decided_at: Option<OffsetDateTime>,
    pub decision_note: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// Body of `POST /dayoff`. Maps to `CreateDayOffCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDayOffRequest {
    pub kind: DayOffKind,
    pub start_date: Date,
    pub end_date: Date,
    #[serde(default)]
    pub start_half: bool,
    #[serde(default)]
    pub end_half: bool,
    #[serde(default)]
    pub reason: String,
}

/// A leader's or HR's decision. Maps to `DecideDayOffCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecideDayOffRequest {
    pub approve: bool,
    #[serde(default)]
    pub note: String,
}

#[cfg(test)]
mod tests {
    use super::DayOffKind;

    #[test]
    fn wire_helpers_match_serde() {
        for k in DayOffKind::ALL {
            assert_eq!(
                serde_json::to_string(&k).unwrap(),
                format!("\"{}\"", k.as_str())
            );
            assert_eq!(DayOffKind::from_wire(k.as_str()), Some(k));
        }
    }
}
