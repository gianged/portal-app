use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

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
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::RequestWork => "Request work",
            Self::Learning => "Learning",
            Self::Other => "Other",
        }
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
    pub report_date: String,
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
    pub note: String,
}
