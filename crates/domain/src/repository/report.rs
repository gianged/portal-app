use async_trait::async_trait;
use time::OffsetDateTime;

use crate::{
    error::RepositoryError,
    ids::{ReportId, UserId},
    model::{
        CompanyStaffStats, GroupProjectStats, GroupRequestStats, GroupStaffStats, MonthlyBucket,
        Period, Report, ReportKind, StaffMonthlyStats, TicketStats,
    },
};

/// Read-only aggregate queries backing the reports. Every method runs a single
/// `GROUP BY` / `COUNT(*) FILTER` scan; the application service joins the results
/// by group and assembles the report structs. `stuck_days` is the staleness
/// threshold for the "stuck" project heuristic.
#[async_trait]
pub trait ReportStatsRepository: Send + Sync {
    async fn project_stats_by_group(
        &self,
        period: Period,
        stuck_days: i32,
    ) -> Result<Vec<GroupProjectStats>, RepositoryError>;

    async fn request_stats_by_group(
        &self,
        period: Period,
    ) -> Result<Vec<GroupRequestStats>, RepositoryError>;

    async fn ticket_stats(&self, period: Period) -> Result<TicketStats, RepositoryError>;

    async fn staff_stats_by_group(
        &self,
        period: Period,
    ) -> Result<Vec<GroupStaffStats>, RepositoryError>;

    async fn company_staff_stats(
        &self,
        period: Period,
    ) -> Result<CompanyStaffStats, RepositoryError>;

    /// Twelve monthly buckets for the given calendar year, oldest first.
    async fn monthly_growth(&self, year: i32) -> Result<Vec<MonthlyBucket>, RepositoryError>;

    /// SQL-derived monthly stats for one staff member over `period`. The service
    /// fills in the work percentage and flex delta from the leave/flex services.
    async fn staff_monthly_stats(
        &self,
        user: UserId,
        period: Period,
    ) -> Result<StaffMonthlyStats, RepositoryError>;
}

/// Archive of generated report artifacts (metadata only; payload in file storage).
#[async_trait]
pub trait ReportArchiveRepository: Send + Sync {
    async fn insert(&self, report: &Report) -> Result<(), RepositoryError>;

    /// Company/group reports, newest first. Staff-scoped artifacts are private
    /// and excluded; they are reached through the staff report path only.
    async fn list(&self, limit: u32) -> Result<Vec<Report>, RepositoryError>;

    async fn find_by_id(&self, id: ReportId) -> Result<Option<Report>, RepositoryError>;

    /// Idempotency guard for the scheduler: a report of this kind already stored
    /// for the period starting at `period_start`.
    async fn find_by_period(
        &self,
        kind: ReportKind,
        period_start: OffsetDateTime,
    ) -> Result<Option<Report>, RepositoryError>;

    /// Idempotency guard for per-staff archival: a staff-scoped report of this
    /// kind already stored for `subject` and the period starting at `period_start`.
    async fn find_by_period_for_subject(
        &self,
        kind: ReportKind,
        period_start: OffsetDateTime,
        subject: UserId,
    ) -> Result<Option<Report>, RepositoryError>;

    /// Every stored report's storage key. Keeps the upload orphan-sweep from
    /// deleting live report artifacts.
    async fn list_all_storage_keys(&self) -> Result<Vec<String>, RepositoryError>;
}
