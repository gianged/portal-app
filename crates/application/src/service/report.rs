use std::{collections::HashMap, sync::Arc};

use domain::{
    error::RenderError,
    ids::{ReportId, UserId},
    model::{
        GroupReportRow, GrowthPoint, GrowthSeries, MonthlyReportData, Period, Report, ReportKind,
        ReportScope, StaffMonthlyReport, StaffSummary, TicketSummary, User, YearlyReportData,
        YearlyTotals,
    },
    ports::{file_storage::FileStorage, report_renderer::ReportRenderer},
    repository::{ReportArchiveRepository, ReportStatsRepository, UserRepository},
};
use time::{Date, Month, OffsetDateTime};
use uuid::Uuid;

use crate::{
    error::{Error, Result},
    permissions::Permissions,
    service::{FlexHoursService, LeaveBalanceService},
};

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

/// Tally of a per-staff archival sweep; one bucket per active user processed.
#[derive(Debug, Clone, Copy, Default)]
pub struct StaffArchiveOutcome {
    pub created: u32,
    pub skipped: u32,
    pub failed: u32,
}

/// Scope and attribution of a stored report: company-wide (optionally credited
/// to the requesting actor) or private to one staff member.
enum Attribution {
    Company { generated_by: Option<UserId> },
    Staff(UserId),
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
    // The per-staff report reuses the leave/flex services for work percentage and
    // flex reconciliation, and `perms` to gate the subject (self / leader / admin).
    leave: Arc<LeaveBalanceService>,
    flex: Arc<FlexHoursService>,
    perms: Arc<Permissions>,
}

impl ReportService {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        stats: Arc<dyn ReportStatsRepository>,
        archive: Arc<dyn ReportArchiveRepository>,
        renderer: Arc<dyn ReportRenderer>,
        storage: Arc<dyn FileStorage>,
        users: Arc<dyn UserRepository>,
        leave: Arc<LeaveBalanceService>,
        flex: Arc<FlexHoursService>,
        perms: Arc<Permissions>,
    ) -> Self {
        Self {
            stats,
            archive,
            renderer,
            storage,
            users,
            leave,
            flex,
            perms,
        }
    }

    /// Aggregated monthly statistics for the dashboard.
    ///
    /// # Errors
    /// Returns `Validation` for an invalid month, or a repository error.
    #[tracing::instrument(skip_all)]
    pub async fn monthly_stats(&self, year: i32, month: u8) -> Result<MonthlyReportData> {
        self.assemble_monthly(month_period(year, month)?).await
    }

    /// Aggregated yearly growth for the dashboard.
    ///
    /// # Errors
    /// Returns `Validation` for an invalid year, or a repository error.
    #[tracing::instrument(skip_all)]
    pub async fn yearly_stats(&self, year: i32) -> Result<YearlyReportData> {
        self.assemble_yearly(year).await
    }

    /// A single staff member's monthly report. The actor may view their own report,
    /// a report for a member of a group they lead, or (as HR/Director) anyone's.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor may not view the subject's report,
    /// `Validation` for an invalid month, or a repository / service error.
    #[tracing::instrument(skip_all, fields(actor = ?actor, subject = ?subject, year, month))]
    pub async fn staff_monthly(
        &self,
        actor: UserId,
        subject: UserId,
        year: i32,
        month: u32,
    ) -> Result<StaffMonthlyReport> {
        if actor != subject {
            match self.perms.require_leader_of_member(actor, subject).await {
                Ok(()) => {}
                Err(Error::Forbidden) => self.perms.require_admin(actor).await?,
                Err(e) => return Err(e),
            }
        }

        let month_u8 =
            u8::try_from(month).map_err(|_| Error::Validation("month must be 1-12".into()))?;
        self.assemble_staff_monthly(subject, month_period(year, month_u8)?)
            .await
    }

    /// Generates (or returns the existing) monthly PDF, storing the artifact.
    /// `generated_by` is `Some(actor)` on demand, `None` for the scheduler.
    ///
    /// # Errors
    /// Returns `Validation` for an invalid month, or a repository, storage, or
    /// render error.
    #[tracing::instrument(skip_all)]
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
    #[tracing::instrument(skip_all)]
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
    #[tracing::instrument(skip_all)]
    pub async fn generate_and_store_monthly(
        &self,
        year: i32,
        month: u8,
    ) -> Result<GeneratedReport> {
        let period = month_period(year, month)?;
        self.store_monthly(period, None).await
    }

    /// Worker entry: idempotently render and archive one staff member's monthly
    /// PDF. Returns `true` when this call created the artifact.
    ///
    /// # Errors
    /// Returns `NotFound` for an unknown subject, `Validation` for an invalid
    /// month, or a repository, storage, or render error.
    #[tracing::instrument(skip_all, fields(subject = ?subject, year, month))]
    pub async fn generate_and_store_staff_monthly(
        &self,
        subject: UserId,
        year: i32,
        month: u8,
    ) -> Result<bool> {
        let user = self
            .users
            .find_by_id(subject)
            .await?
            .ok_or(Error::NotFound("user"))?;
        self.store_staff_monthly(&user, month_period(year, month)?)
            .await
    }

    /// Worker entry: archive the monthly PDF for every active user. Idempotent;
    /// a per-user failure is logged and counted, never sinks the sweep.
    ///
    /// # Errors
    /// Returns `Validation` for an invalid month, or a repository error from the
    /// user listing itself.
    #[tracing::instrument(skip_all, fields(year, month))]
    pub async fn archive_staff_monthly_reports(
        &self,
        year: i32,
        month: u8,
    ) -> Result<StaffArchiveOutcome> {
        const PAGE: u32 = 200;
        let period = month_period(year, month)?;
        let mut outcome = StaffArchiveOutcome::default();
        let mut offset = 0_u32;
        loop {
            let page = self.users.list_active(PAGE, offset, None).await?;
            let page_len = page.len();
            for user in page {
                match self.store_staff_monthly(&user, period).await {
                    Ok(true) => outcome.created += 1,
                    Ok(false) => outcome.skipped += 1,
                    Err(e) => {
                        outcome.failed += 1;
                        tracing::error!(error = %e, subject = ?user.id, "staff report archival failed");
                    }
                }
            }
            if page_len < PAGE as usize {
                break;
            }
            offset += PAGE;
        }
        Ok(outcome)
    }

    /// Lists archived company/group reports, newest first. Staff-scoped
    /// artifacts are excluded; they are private to the subject's report path.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(limit = ?limit))]
    pub async fn list_reports(&self, limit: u32) -> Result<Vec<Report>> {
        Ok(self.archive.list(limit).await?)
    }

    /// Director/HR recipients for the scheduled report mail.
    ///
    /// # Errors
    /// Returns a repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all)]
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
                Attribution::Company { generated_by },
                storage_key,
                bytes.clone(),
            )
            .await?;
        Ok(GeneratedReport {
            report,
            bytes,
            created: true,
        })
    }

    /// Idempotently renders and archives one staff member's monthly PDF; `true`
    /// when this call created the artifact, `false` when it already existed.
    async fn store_staff_monthly(&self, subject: &User, period: Period) -> Result<bool> {
        if self
            .archive
            .find_by_period_for_subject(ReportKind::Monthly, period.start, subject.id)
            .await?
            .is_some()
        {
            return Ok(false);
        }
        let data = self.assemble_staff_monthly(subject.id, period).await?;
        let renderer = self.renderer.clone();
        let name = subject.full_name.clone();
        let bytes =
            tokio::task::spawn_blocking(move || renderer.render_staff_monthly(&name, &data))
                .await
                .map_err(|e| Error::Render(RenderError::Backend(e.to_string())))??;
        let (y, m) = (period.start.year(), u8::from(period.start.month()));
        let storage_key = format!("reports/monthly/{y:04}-{m:02}/staff/{}.pdf", subject.id.0);
        self.persist(
            period,
            ReportKind::Monthly,
            Attribution::Staff(subject.id),
            storage_key,
            bytes,
        )
        .await?;
        Ok(true)
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
                Attribution::Company { generated_by },
                storage_key,
                bytes.clone(),
            )
            .await?;
        Ok(GeneratedReport {
            report,
            bytes,
            created: true,
        })
    }

    /// Stores the PDF bytes and inserts the archive row.
    async fn persist(
        &self,
        period: Period,
        kind: ReportKind,
        attribution: Attribution,
        storage_key: String,
        bytes: Vec<u8>,
    ) -> Result<Report> {
        self.storage
            .put(&storage_key, CONTENT_TYPE, bytes.clone())
            .await?;
        let (scope, subject, generated_by) = match attribution {
            Attribution::Company { generated_by } => (ReportScope::Company, None, generated_by),
            Attribution::Staff(subject) => (ReportScope::Staff, Some(subject), None),
        };
        let report = Report {
            id: ReportId(Uuid::now_v7()),
            kind,
            scope,
            group_id: None,
            subject_user_id: subject,
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

    /// Assembles one user's monthly report without an authorization gate; callers
    /// either check the actor (`staff_monthly`) or run in system context.
    async fn assemble_staff_monthly(
        &self,
        subject: UserId,
        period: Period,
    ) -> Result<StaffMonthlyReport> {
        let year = period.start.year();
        let month = u32::from(u8::from(period.start.month()));
        // Balance as of the last day of the report month.
        let asof = period
            .end
            .date()
            .previous_day()
            .ok_or_else(|| Error::Validation("invalid report period".into()))?;

        let stats = self.stats.staff_monthly_stats(subject, period).await?;
        let work_percentage = pct(self.leave.work_percentage(subject, year, month).await?);
        let flex_month_delta = self.flex.month_delta(subject, year, month).await?;
        let balance_remaining = self.leave.available(subject, asof).await?;

        Ok(StaffMonthlyReport {
            user_id: subject,
            period,
            days_reported: stats.days_reported,
            hours_request_work: stats.hours_request_work,
            hours_learning: stats.hours_learning,
            hours_other: stats.hours_other,
            leave_days_by_kind: stats.leave_days_by_kind,
            overtime_hours: stats.overtime_hours,
            flex_days: stats.flex_days,
            flex_month_delta,
            work_percentage,
            balance_remaining,
            balance_expiring_soon: stats.balance_expiring_soon,
            requests_completed: stats.requests_completed,
            requests_open: stats.requests_open,
            avg_request_progress: stats.avg_request_progress,
        })
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

/// Round and clamp a percentage value into the 0-100 range.
fn pct(v: f64) -> u8 {
    let r = v.round();
    if r <= 0.0 {
        0
    } else if r >= 100.0 {
        100
    } else {
        r as u8
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
