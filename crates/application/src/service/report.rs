use std::{collections::HashMap, sync::Arc};

use domain::{
    error::RenderError,
    ids::{ReportId, UserId},
    model::{
        GroupReportRow, GrowthPoint, GrowthSeries, MonthlyReportData, Period, Report, ReportKind,
        ReportScope, StaffSummary, TicketSummary, YearlyReportData, YearlyTotals,
    },
    ports::{file_storage::FileStorage, report_renderer::ReportRenderer},
    repository::{ReportArchiveRepository, ReportStatsRepository, UserRepository},
};
use time::{Date, Month, OffsetDateTime};
use uuid::Uuid;

use crate::error::{Error, Result};

/// A project counts as "stuck" if on hold, or active with no update in this many days.
const STUCK_DAYS: i32 = 14;
const CONTENT_TYPE: &str = "application/pdf";

/// Outcome of a store-and-generate call: the metadata, the PDF bytes, and whether
/// this call actually created the artifact (vs returning an existing one).
pub struct GeneratedReport {
    pub report: Report,
    pub bytes: Vec<u8>,
    pub created: bool,
}

/// Aggregates report data, renders PDFs, and archives the artifacts.
///
/// System-level: it performs no authorization. The Director/HR gate lives at the
/// call site; `generated_by` on the generate methods records attribution only.
pub struct ReportService {
    stats: Arc<dyn ReportStatsRepository>,
    archive: Arc<dyn ReportArchiveRepository>,
    renderer: Arc<dyn ReportRenderer>,
    storage: Arc<dyn FileStorage>,
    users: Arc<dyn UserRepository>,
}

impl ReportService {
    #[must_use]
    pub fn new(
        stats: Arc<dyn ReportStatsRepository>,
        archive: Arc<dyn ReportArchiveRepository>,
        renderer: Arc<dyn ReportRenderer>,
        storage: Arc<dyn FileStorage>,
        users: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            stats,
            archive,
            renderer,
            storage,
            users,
        }
    }

    /// Aggregated monthly statistics for the dashboard.
    ///
    /// # Errors
    /// Returns `Validation` for an invalid month, or a repository error.
    pub async fn monthly_stats(&self, year: i32, month: u8) -> Result<MonthlyReportData> {
        self.assemble_monthly(month_period(year, month)?).await
    }

    /// Aggregated yearly growth for the dashboard.
    ///
    /// # Errors
    /// Returns `Validation` for an invalid year, or a repository error.
    pub async fn yearly_stats(&self, year: i32) -> Result<YearlyReportData> {
        self.assemble_yearly(year).await
    }

    /// Generates (or returns the existing) monthly PDF, storing the artifact.
    /// `generated_by` is `Some(actor)` on demand, `None` for the scheduler.
    ///
    /// # Errors
    /// Returns `Validation` for an invalid month, or a repository, storage, or
    /// render error.
    pub async fn generate_monthly(
        &self,
        year: i32,
        month: u8,
        generated_by: Option<UserId>,
    ) -> Result<Report> {
        let period = month_period(year, month)?;
        Ok(self.store_monthly(period, generated_by).await?.report)
    }

    /// Generates (or returns the existing) yearly PDF, storing the artifact.
    ///
    /// # Errors
    /// Returns `Validation` for an invalid year, or a repository, storage, or
    /// render error.
    pub async fn generate_yearly(&self, year: i32, generated_by: Option<UserId>) -> Result<Report> {
        let period = year_period(year)?;
        Ok(self.store_yearly(period, year, generated_by).await?.report)
    }

    /// Worker entry: idempotently generate and store the company monthly report.
    /// The returned `created` flag lets the scheduler email only on first creation.
    ///
    /// # Errors
    /// Returns `Validation` for an invalid month, or a repository, storage, or
    /// render error.
    pub async fn generate_and_store_monthly(
        &self,
        year: i32,
        month: u8,
    ) -> Result<GeneratedReport> {
        let period = month_period(year, month)?;
        self.store_monthly(period, None).await
    }

    /// Lists archived reports, newest first.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    pub async fn list_reports(&self, limit: u32) -> Result<Vec<Report>> {
        Ok(self.archive.list(limit).await?)
    }

    /// Director/HR recipients for the scheduled report mail.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    pub async fn list_admin_recipients(&self) -> Result<Vec<(String, UserId)>> {
        let users = self.users.list_with_system_role().await?;
        Ok(users.into_iter().map(|u| (u.email, u.id)).collect())
    }

    // --- internals ---

    async fn store_monthly(
        &self,
        period: Period,
        generated_by: Option<UserId>,
    ) -> Result<GeneratedReport> {
        if let Some(report) = self
            .archive
            .find_by_period(ReportKind::Monthly, period.start)
            .await?
        {
            let bytes = self.storage.get(&report.storage_key).await?;
            return Ok(GeneratedReport {
                report,
                bytes,
                created: false,
            });
        }
        let data = self.assemble_monthly(period).await?;
        let renderer = self.renderer.clone();
        let bytes = tokio::task::spawn_blocking(move || renderer.render_monthly(&data))
            .await
            .map_err(|e| Error::Render(RenderError::Backend(e.to_string())))??;
        let (y, m) = (period.start.year(), u8::from(period.start.month()));
        let storage_key = format!("reports/monthly/{y:04}-{m:02}/company.pdf");
        let report = self
            .persist(
                period,
                ReportKind::Monthly,
                storage_key,
                bytes.clone(),
                generated_by,
            )
            .await?;
        Ok(GeneratedReport {
            report,
            bytes,
            created: true,
        })
    }

    async fn store_yearly(
        &self,
        period: Period,
        year: i32,
        generated_by: Option<UserId>,
    ) -> Result<GeneratedReport> {
        if let Some(report) = self
            .archive
            .find_by_period(ReportKind::Yearly, period.start)
            .await?
        {
            let bytes = self.storage.get(&report.storage_key).await?;
            return Ok(GeneratedReport {
                report,
                bytes,
                created: false,
            });
        }
        let data = self.assemble_yearly(year).await?;
        let renderer = self.renderer.clone();
        let bytes = tokio::task::spawn_blocking(move || renderer.render_yearly(&data))
            .await
            .map_err(|e| Error::Render(RenderError::Backend(e.to_string())))??;
        let storage_key = format!("reports/yearly/{year:04}/company.pdf");
        let report = self
            .persist(
                period,
                ReportKind::Yearly,
                storage_key,
                bytes.clone(),
                generated_by,
            )
            .await?;
        Ok(GeneratedReport {
            report,
            bytes,
            created: true,
        })
    }

    async fn persist(
        &self,
        period: Period,
        kind: ReportKind,
        storage_key: String,
        bytes: Vec<u8>,
        generated_by: Option<UserId>,
    ) -> Result<Report> {
        self.storage
            .put(&storage_key, CONTENT_TYPE, bytes.clone())
            .await?;
        let report = Report {
            id: ReportId(Uuid::now_v7()),
            kind,
            scope: ReportScope::Company,
            group_id: None,
            period_start: period.start,
            period_end: period.end,
            storage_key,
            content_type: CONTENT_TYPE.to_owned(),
            size_bytes: bytes.len() as u64,
            generated_by,
            generated_at: OffsetDateTime::now_utc(),
        };
        self.archive.insert(&report).await?;
        Ok(report)
    }

    async fn assemble_monthly(&self, period: Period) -> Result<MonthlyReportData> {
        let projects = self
            .stats
            .project_stats_by_group(period, STUCK_DAYS)
            .await?;
        let requests = self.stats.request_stats_by_group(period).await?;
        let staff = self.stats.staff_stats_by_group(period).await?;
        let tickets = self.stats.ticket_stats(period).await?;
        let company = self.stats.company_staff_stats(period).await?;

        let req_by_group: HashMap<_, _> = requests.iter().map(|r| (r.group_id, r)).collect();
        let staff_by_group: HashMap<_, _> = staff.iter().map(|s| (s.group_id, s)).collect();

        let mut groups = Vec::with_capacity(projects.len());
        let mut per_group = Vec::with_capacity(projects.len());
        for p in &projects {
            let r = req_by_group.get(&p.group_id).copied();
            let headcount = staff_by_group.get(&p.group_id).map_or(0, |s| s.headcount);
            let requests_total = r.map_or(0, |r| r.total);
            let requests_completed = r.map_or(0, |r| r.completed);
            let requests_open = r.map_or(0, |r| r.open);
            let request_completion_pct = if requests_total > 0 {
                ((f64::from(requests_completed) / f64::from(requests_total)) * 100.0).round() as u8
            } else {
                0
            };
            groups.push(GroupReportRow {
                group_id: p.group_id,
                group_name: p.group_name.clone(),
                group_kind: p.group_kind,
                projects_total: p.total,
                projects_completed: p.completed,
                projects_active: p.active,
                projects_on_hold: p.on_hold,
                projects_planning: p.planning,
                projects_cancelled: p.cancelled,
                projects_stuck: p.stuck,
                avg_project_progress: p.avg_progress,
                requests_total,
                requests_completed,
                requests_open,
                request_completion_pct,
                headcount,
            });
            per_group.push((p.group_id, p.group_name.clone(), headcount));
        }

        let ticket_summary = TicketSummary {
            created_in_period: tickets.created_in_period,
            resolved_in_period: tickets.resolved_in_period,
            by_status: tickets.by_status,
            by_category: tickets.by_category,
            avg_resolve_hours: tickets.avg_resolve_hours,
        };
        let staff_summary = StaffSummary {
            company_headcount: company.active_users,
            new_joiners: company.new_active_users,
            deactivations: company.deactivated_users,
            per_group,
        };

        Ok(MonthlyReportData {
            period,
            groups,
            tickets: ticket_summary,
            staff: staff_summary,
        })
    }

    async fn assemble_yearly(&self, year: i32) -> Result<YearlyReportData> {
        let buckets = self.stats.monthly_growth(year).await?;
        let company = self.stats.company_staff_stats(year_period(year)?).await?;

        let mut growth = GrowthSeries {
            headcount: Vec::new(),
            new_joiners: Vec::new(),
            tickets_created: Vec::new(),
            projects_completed: Vec::new(),
            requests_completed: Vec::new(),
        };
        let (mut new_hires, mut departures) = (0_u32, 0_u32);
        let (mut tickets_created, mut projects_completed, mut requests_completed) =
            (0_u32, 0_u32, 0_u32);
        let mut net = 0_i32;
        for b in &buckets {
            let point = |value: i64| GrowthPoint {
                year: b.year,
                month: b.month,
                value,
            };
            growth
                .headcount
                .push(point(i64::from(b.headcount_delta_cum)));
            growth.new_joiners.push(point(i64::from(b.new_joiners)));
            growth
                .tickets_created
                .push(point(i64::from(b.tickets_created)));
            growth
                .projects_completed
                .push(point(i64::from(b.projects_completed)));
            growth
                .requests_completed
                .push(point(i64::from(b.requests_completed)));
            new_hires += b.new_joiners;
            departures += b.deactivations;
            tickets_created += b.tickets_created;
            projects_completed += b.projects_completed;
            requests_completed += b.requests_completed;
            net = b.headcount_delta_cum;
        }

        Ok(YearlyReportData {
            year,
            growth,
            totals: YearlyTotals {
                company_headcount: company.active_users,
                net_headcount_change: net,
                new_hires,
                departures,
                tickets_created,
                projects_completed,
                requests_completed,
            },
        })
    }
}

fn month_enum(month: u8) -> Result<Month> {
    Month::try_from(month).map_err(|_| Error::Validation("month must be 1-12".into()))
}

fn first_of_month(year: i32, month: u8) -> Result<OffsetDateTime> {
    Ok(Date::from_calendar_date(year, month_enum(month)?, 1)
        .map_err(|e| Error::Validation(e.to_string()))?
        .midnight()
        .assume_utc())
}

fn month_period(year: i32, month: u8) -> Result<Period> {
    let start = first_of_month(year, month)?;
    let (ny, nm) = if month >= 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let end = first_of_month(ny, nm)?;
    Ok(Period { start, end })
}

fn year_period(year: i32) -> Result<Period> {
    Ok(Period {
        start: first_of_month(year, 1)?,
        end: first_of_month(year + 1, 1)?,
    })
}
