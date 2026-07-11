use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::dto::{
    common::UserSummaryDto,
    ids::{DailyReportEntryId, DailyReportId, RequestId},
};

/// Mirrors `domain::model::DailyReportStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DailyReportStatus {
    Draft,
    Submitted,
    Approved,
    Returned,
}

impl DailyReportStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Submitted => "Submitted",
            Self::Approved => "Approved",
            Self::Returned => "Returned",
        }
    }
}

/// Mirrors `domain::model::DailyReportEntryKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DailyReportEntryKind {
    RequestWork,
    Learning,
    Other,
}

impl DailyReportEntryKind {
    /// Every variant, for building select options.
    pub const ALL: [Self; 3] = [Self::RequestWork, Self::Learning, Self::Other];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::RequestWork => "Request work",
            Self::Learning => "Learning",
            Self::Other => "Other",
        }
    }

    /// Canonical wire string (the serde `snake_case` tag).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RequestWork => "request_work",
            Self::Learning => "learning",
            Self::Other => "other",
        }
    }

    /// Parses a wire string produced by [`Self::as_str`].
    #[must_use]
    pub fn from_wire(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportEntryDto {
    pub id: DailyReportEntryId,
    pub kind: DailyReportEntryKind,
    pub description: String,
    pub request_id: Option<RequestId>,
    pub hours: Option<f64>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// A daily report with its owner, entries, and review metadata. `report_date` is
/// wire-encoded as `"YYYY-MM-DD"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReportDto {
    pub id: DailyReportId,
    pub user: UserSummaryDto,
    pub report_date: Date,
    pub status: DailyReportStatus,
    pub summary: String,
    pub entries: Vec<DailyReportEntryDto>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub submitted_at: Option<OffsetDateTime>,
    pub reviewed_by: Option<UserSummaryDto>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub reviewed_at: Option<OffsetDateTime>,
    pub review_note: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// One line of a `PUT /daily-reports/{date}` body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertDailyReportEntry {
    pub kind: DailyReportEntryKind,
    pub description: String,
    pub request_id: Option<RequestId>,
    pub hours: Option<f64>,
    /// Optional completion hint for a `RequestWork` entry; bumps the linked request.
    pub progress: Option<u8>,
}

/// Create-or-replace a draft report. Maps to `UpsertDailyReportCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertDailyReportRequest {
    pub summary: String,
    pub entries: Vec<UpsertDailyReportEntry>,
}

/// A leader's decision. Maps to `ReviewDailyReportCommand`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewDailyReportRequest {
    pub approve: bool,
    #[serde(default)]
    pub note: String,
}

#[cfg(test)]
mod tests {
    use super::DailyReportEntryKind;

    #[test]
    fn wire_helpers_match_serde() {
        for k in DailyReportEntryKind::ALL {
            assert_eq!(
                serde_json::to_string(&k).unwrap(),
                format!("\"{}\"", k.as_str())
            );
            assert_eq!(DailyReportEntryKind::from_wire(k.as_str()), Some(k));
        }
    }
}
