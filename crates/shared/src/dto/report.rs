use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::ids::{GroupId, ReportId};

/// Mirrors `domain::model::ReportKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportKindDto {
    Monthly,
    Yearly,
}

impl ReportKindDto {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Monthly => "Monthly",
            Self::Yearly => "Yearly",
        }
    }
}

/// A labelled count, used for ticket status / category breakdowns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabeledCountDto {
    pub label: String,
    pub count: u32,
}

/// One group's line in the monthly report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupReportRowDto {
    pub group_id: GroupId,
    pub group_name: String,
    pub is_it: bool,
    pub projects_total: u32,
    pub projects_completed: u32,
    pub projects_active: u32,
    pub projects_on_hold: u32,
    pub projects_stuck: u32,
    pub avg_project_progress: u8,
    pub requests_total: u32,
    pub requests_completed: u32,
    pub requests_open: u32,
    pub request_completion_pct: u8,
    pub headcount: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketSummaryDto {
    pub created_in_period: u32,
    pub resolved_in_period: u32,
    pub avg_resolve_hours: Option<f64>,
    pub by_status: Vec<LabeledCountDto>,
    pub by_category: Vec<LabeledCountDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupHeadcountDto {
    pub group_id: GroupId,
    pub group_name: String,
    pub headcount: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaffSummaryDto {
    pub company_headcount: u32,
    pub new_joiners: u32,
    pub deactivations: u32,
    pub per_group: Vec<GroupHeadcountDto>,
}

/// Aggregated monthly statistics for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyReportDto {
    pub year: i32,
    pub month: u8,
    pub groups: Vec<GroupReportRowDto>,
    pub tickets: TicketSummaryDto,
    pub staff: StaffSummaryDto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GrowthPointDto {
    pub year: i32,
    pub month: u8,
    pub value: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthSeriesDto {
    pub headcount: Vec<GrowthPointDto>,
    pub new_joiners: Vec<GrowthPointDto>,
    pub tickets_created: Vec<GrowthPointDto>,
    pub projects_completed: Vec<GrowthPointDto>,
    pub requests_completed: Vec<GrowthPointDto>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct YearlyTotalsDto {
    pub company_headcount: u32,
    pub net_headcount_change: i32,
    pub new_hires: u32,
    pub departures: u32,
    pub tickets_created: u32,
    pub projects_completed: u32,
    pub requests_completed: u32,
}

/// Aggregated yearly growth for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YearlyReportDto {
    pub year: i32,
    pub growth: GrowthSeriesDto,
    pub totals: YearlyTotalsDto,
}

/// An archived report artifact with a ready-to-use signed download URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSummaryDto {
    pub id: ReportId,
    pub kind: ReportKindDto,
    #[serde(with = "time::serde::rfc3339")]
    pub period_start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub period_end: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub generated_at: OffsetDateTime,
    pub size_bytes: u64,
    pub download_url: String,
}
