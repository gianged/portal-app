use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{
    ids::{GroupId, ReportId, UserId},
    model::{DayOffKind, GroupKind, TicketCategory, TicketStatus},
};

/// Half-open reporting window `[start, end)`. Boundaries are the first instant of the
/// period and of the next, comparing cleanly against the timestamps queries filter on.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Period {
    pub start: OffsetDateTime,
    pub end: OffsetDateTime,
}

// -----------------------------------------------------------------------------
// Granular read-models: one query each from the aggregation repository. The
// application service joins these by group into the renderer-facing structs below.
// -----------------------------------------------------------------------------

/// Per-group project counts (one row per group, zero-filled for empty groups).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupProjectStats {
    pub group_id: GroupId,
    pub group_name: String,
    pub group_kind: GroupKind,
    pub total: u32,
    pub planning: u32,
    pub active: u32,
    pub on_hold: u32,
    pub completed: u32,
    pub cancelled: u32,
    /// Average `progress` over non-terminal projects (rounded).
    pub avg_progress: u8,
    /// On-hold projects plus active projects with no update in the stuck window.
    pub stuck: u32,
}

/// Per-group request counts (requests reach a group via their project's owner).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupRequestStats {
    pub group_id: GroupId,
    pub total: u32,
    pub completed: u32,
    pub open: u32,
}

/// IT ticket aggregates over the period. Tickets are org-wide, not group-scoped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketStats {
    pub created_in_period: u32,
    pub resolved_in_period: u32,
    pub by_status: Vec<(TicketStatus, u32)>,
    pub by_category: Vec<(TicketCategory, u32)>,
    /// Mean time-to-resolve for tickets resolved in the period, in hours.
    pub avg_resolve_hours: Option<f64>,
}

/// Per-group staffing as-of the period end.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupStaffStats {
    pub group_id: GroupId,
    pub headcount: u32,
}

/// Company-wide staffing (user-lifecycle based, distinct from membership flow).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CompanyStaffStats {
    pub active_users: u32,
    pub new_active_users: u32,
    pub deactivated_users: u32,
}

/// One month of the year-over-year series.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MonthlyBucket {
    pub year: i32,
    pub month: u8,
    pub new_joiners: u32,
    pub deactivations: u32,
    /// Running net headcount change within the year (cumulative joiners - leavers).
    pub headcount_delta_cum: i32,
    pub tickets_created: u32,
    pub projects_completed: u32,
    pub requests_completed: u32,
}

// -----------------------------------------------------------------------------
// Renderer-facing structs: assembled by the application service, consumed by the
// PDF renderer and mapped to DTOs by the server.
// -----------------------------------------------------------------------------

/// One group's line in the monthly report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupReportRow {
    pub group_id: GroupId,
    pub group_name: String,
    pub group_kind: GroupKind,
    // projects
    pub projects_total: u32,
    pub projects_completed: u32,
    pub projects_active: u32,
    pub projects_on_hold: u32,
    pub projects_planning: u32,
    pub projects_cancelled: u32,
    pub projects_stuck: u32,
    pub avg_project_progress: u8,
    // requests
    pub requests_total: u32,
    pub requests_completed: u32,
    pub requests_open: u32,
    pub request_completion_pct: u8,
    // staff
    pub headcount: u32,
}

/// Company staffing roll-up for the report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaffSummary {
    pub company_headcount: u32,
    pub new_joiners: u32,
    pub deactivations: u32,
    /// (group id, name, headcount) for the per-group breakdown.
    pub per_group: Vec<(GroupId, String, u32)>,
}

/// One point in a year-over-year growth series.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GrowthPoint {
    pub year: i32,
    pub month: u8,
    pub value: i64,
}

/// The full set of monthly growth series for the yearly view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrowthSeries {
    pub headcount: Vec<GrowthPoint>,
    pub new_joiners: Vec<GrowthPoint>,
    pub tickets_created: Vec<GrowthPoint>,
    pub projects_completed: Vec<GrowthPoint>,
    pub requests_completed: Vec<GrowthPoint>,
}

/// Headline yearly totals: "is the company growing?" in a handful of numbers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct YearlyTotals {
    pub company_headcount: u32,
    pub net_headcount_change: i32,
    pub new_hires: u32,
    pub departures: u32,
    pub tickets_created: u32,
    pub projects_completed: u32,
    pub requests_completed: u32,
}

/// Everything the monthly report needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyReportData {
    pub period: Period,
    pub groups: Vec<GroupReportRow>,
    pub tickets: TicketStats,
    pub staff: StaffSummary,
}

/// Everything the yearly view/report needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YearlyReportData {
    pub year: i32,
    pub growth: GrowthSeries,
    pub totals: YearlyTotals,
}

// -----------------------------------------------------------------------------
// Per-staff monthly report: aggregates one user's attendance + workload. The SQL
// stats carry the query-derived parts; the application service fills the rest from
// the leave/flex services.
// -----------------------------------------------------------------------------

/// SQL-derived parts of a staff member's monthly report. `work_percentage` and
/// `flex_month_delta` are computed by the application service, not here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaffMonthlyStats {
    /// Distinct dates with an approved daily report in the month.
    pub days_reported: u32,
    pub hours_request_work: f64,
    pub hours_learning: f64,
    pub hours_other: f64,
    /// Approved leave days summed per kind (only kinds with days are present).
    pub leave_days_by_kind: Vec<(DayOffKind, f64)>,
    pub overtime_hours: f64,
    /// Count of approved flex days in the month.
    pub flex_days: u32,
    /// Remaining balance on grants expiring within the report month.
    pub balance_expiring_soon: f64,
    pub requests_completed: u32,
    pub requests_open: u32,
    pub avg_request_progress: u8,
}

/// A single staff member's monthly attendance + workload roll-up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaffMonthlyReport {
    pub user_id: UserId,
    pub period: Period,
    pub days_reported: u32,
    pub hours_request_work: f64,
    pub hours_learning: f64,
    pub hours_other: f64,
    pub leave_days_by_kind: Vec<(DayOffKind, f64)>,
    pub overtime_hours: f64,
    pub flex_days: u32,
    /// Net deviation of approved flex hours from the expected monthly total; 0 = reconciled.
    pub flex_month_delta: f64,
    /// Present working days over expected working days, as a percentage.
    pub work_percentage: u8,
    pub balance_remaining: f64,
    pub balance_expiring_soon: f64,
    pub requests_completed: u32,
    pub requests_open: u32,
    pub avg_request_progress: u8,
}

// -----------------------------------------------------------------------------
// Archive entity: metadata for a stored, generated report artifact.
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportScope {
    Company,
    Group,
    /// One user's monthly attendance + workload archive.
    Staff,
}

/// A generated report whose PDF payload lives in file storage under `storage_key`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub id: ReportId,
    pub kind: ReportKind,
    pub scope: ReportScope,
    pub group_id: Option<GroupId>,
    /// `Some` iff `scope` is `Staff`.
    pub subject_user_id: Option<UserId>,
    pub period_start: OffsetDateTime,
    pub period_end: OffsetDateTime,
    pub storage_key: String,
    pub content_type: String,
    pub size_bytes: u64,
    /// `None` when produced by the scheduled job (system context).
    pub generated_by: Option<UserId>,
    pub generated_at: OffsetDateTime,
}
