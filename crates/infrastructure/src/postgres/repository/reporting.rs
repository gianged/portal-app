use async_trait::async_trait;
use sqlx::PgPool;
use time::{Date, Month, OffsetDateTime};
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{GroupId, ReportId, UserId},
    model::{
        CompanyStaffStats, DayOffKind, GroupProjectStats, GroupRequestStats, GroupStaffStats,
        MonthlyBucket, Period, Report, ReportKind, StaffMonthlyStats, TicketCategory, TicketStats,
        TicketStatus,
    },
    repository::{ReportArchiveRepository, ReportStatsRepository},
};

use crate::postgres::{
    enums::{SqlDayOffKind, SqlGroupKind, SqlReportKind, SqlReportScope},
    mappers,
};

pub struct PgReportingRepo {
    pool: PgPool,
}

impl PgReportingRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// COUNT columns come back as `i64`; counts are non-negative and small.
fn count(n: i64) -> u32 {
    u32::try_from(n).unwrap_or(0)
}

/// Round and clamp an average into the 0-100 percentage range.
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

// -----------------------------------------------------------------------------
// Aggregate read-models
// -----------------------------------------------------------------------------

struct ProjectStatsRow {
    group_id: Uuid,
    group_name: String,
    group_kind: SqlGroupKind,
    total: i64,
    planning: i64,
    active: i64,
    on_hold: i64,
    completed: i64,
    cancelled: i64,
    avg_progress: f64,
    stuck: i64,
}

struct RequestStatsRow {
    group_id: Uuid,
    total: i64,
    completed: i64,
    cancelled: i64,
    open: i64,
}

struct TicketStatsRow {
    created_in_period: i64,
    open: i64,
    triaged: i64,
    assigned: i64,
    in_progress: i64,
    resolved: i64,
    closed: i64,
    reopened: i64,
    cat_hardware: i64,
    cat_software: i64,
    cat_access: i64,
    cat_other: i64,
    resolved_in_period: i64,
    avg_resolve_secs: Option<f64>,
}

struct StaffStatsRow {
    group_id: Uuid,
    headcount: i64,
    new_joiners: i64,
    deactivations: i64,
}

struct MonthlyBucketRow {
    month: OffsetDateTime,
    new_joiners: i64,
    deactivations: i64,
    headcount_delta_cum: i64,
    tickets_created: i64,
    projects_completed: i64,
    requests_completed: i64,
}

struct ReportRow {
    id: Uuid,
    kind: SqlReportKind,
    scope: SqlReportScope,
    group_id: Option<Uuid>,
    period_start: OffsetDateTime,
    period_end: OffsetDateTime,
    storage_key: String,
    content_type: String,
    size_bytes: i64,
    generated_by: Option<Uuid>,
    generated_at: OffsetDateTime,
}

impl TryFrom<ReportRow> for Report {
    type Error = RepositoryError;

    fn try_from(r: ReportRow) -> Result<Self, Self::Error> {
        let size_bytes = u64::try_from(r.size_bytes)
            .map_err(|_| RepositoryError::Backend("negative size_bytes in report row".into()))?;
        Ok(Self {
            id: ReportId(r.id),
            kind: r.kind.into(),
            scope: r.scope.into(),
            group_id: r.group_id.map(GroupId),
            period_start: r.period_start,
            period_end: r.period_end,
            storage_key: r.storage_key,
            content_type: r.content_type,
            size_bytes,
            generated_by: r.generated_by.map(UserId),
            generated_at: r.generated_at,
        })
    }
}

#[async_trait]
impl ReportStatsRepository for PgReportingRepo {
    #[tracing::instrument(skip_all)]
    async fn project_stats_by_group(
        &self,
        period: Period,
        stuck_days: i32,
    ) -> Result<Vec<GroupProjectStats>, RepositoryError> {
        let rows = sqlx::query_as!(
            ProjectStatsRow,
            r#"SELECT
                 g.id   AS "group_id!",
                 g.name AS "group_name!",
                 g.kind AS "group_kind!: SqlGroupKind",
                 COUNT(p.id)                                    AS "total!",
                 COUNT(*) FILTER (WHERE p.status = 'planning')  AS "planning!",
                 COUNT(*) FILTER (WHERE p.status = 'active')    AS "active!",
                 COUNT(*) FILTER (WHERE p.status = 'on_hold')   AS "on_hold!",
                 COUNT(*) FILTER (WHERE p.status = 'completed') AS "completed!",
                 COUNT(*) FILTER (WHERE p.status = 'cancelled') AS "cancelled!",
                 COALESCE(
                   AVG(p.progress) FILTER (WHERE p.status NOT IN ('completed','cancelled')),
                   0
                 )::float8                                      AS "avg_progress!",
                 COUNT(*) FILTER (
                   WHERE p.status = 'on_hold'
                      OR (p.status = 'active'
                          AND p.updated_at < $1::timestamptz - make_interval(days => $2::int))
                 )                                              AS "stuck!"
               FROM org.groups g
               LEFT JOIN project.projects p
                      ON p.owner_group_id = g.id
                     AND p.created_at < $1::timestamptz
               GROUP BY g.id
               ORDER BY g.name"#,
            period.end,
            stuck_days,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;

        Ok(rows
            .into_iter()
            .map(|r| GroupProjectStats {
                group_id: GroupId(r.group_id),
                group_name: r.group_name,
                group_kind: r.group_kind.into(),
                total: count(r.total),
                planning: count(r.planning),
                active: count(r.active),
                on_hold: count(r.on_hold),
                completed: count(r.completed),
                cancelled: count(r.cancelled),
                avg_progress: pct(r.avg_progress),
                stuck: count(r.stuck),
            })
            .collect())
    }

    #[tracing::instrument(skip_all)]
    async fn request_stats_by_group(
        &self,
        period: Period,
    ) -> Result<Vec<GroupRequestStats>, RepositoryError> {
        let rows = sqlx::query_as!(
            RequestStatsRow,
            r#"SELECT
                 g.id AS "group_id!",
                 COUNT(r.id)                                    AS "total!",
                 COUNT(*) FILTER (WHERE r.status = 'completed') AS "completed!",
                 COUNT(*) FILTER (WHERE r.status = 'cancelled') AS "cancelled!",
                 COUNT(*) FILTER (
                   WHERE r.status IN ('draft','submitted','assigned','in_progress','review')
                 )                                              AS "open!"
               FROM org.groups g
               LEFT JOIN project.projects p ON p.owner_group_id = g.id
               LEFT JOIN project.requests  r ON r.project_id = p.id
                                            AND r.created_at < $1::timestamptz
               GROUP BY g.id
               ORDER BY g.id"#,
            period.end,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;

        Ok(rows
            .into_iter()
            .map(|r| GroupRequestStats {
                group_id: GroupId(r.group_id),
                total: count(r.total),
                completed: count(r.completed),
                cancelled: count(r.cancelled),
                open: count(r.open),
            })
            .collect())
    }

    #[tracing::instrument(skip_all)]
    async fn ticket_stats(&self, period: Period) -> Result<TicketStats, RepositoryError> {
        let r = sqlx::query_as!(
            TicketStatsRow,
            r#"SELECT
                 COUNT(*) FILTER (WHERE t.created_at >= $2 AND t.created_at < $1) AS "created_in_period!",
                 COUNT(*) FILTER (WHERE t.status = 'open')        AS "open!",
                 COUNT(*) FILTER (WHERE t.status = 'triaged')     AS "triaged!",
                 COUNT(*) FILTER (WHERE t.status = 'assigned')    AS "assigned!",
                 COUNT(*) FILTER (WHERE t.status = 'in_progress') AS "in_progress!",
                 COUNT(*) FILTER (WHERE t.status = 'resolved')    AS "resolved!",
                 COUNT(*) FILTER (WHERE t.status = 'closed')      AS "closed!",
                 COUNT(*) FILTER (WHERE t.status = 'reopened')    AS "reopened!",
                 COUNT(*) FILTER (WHERE t.category = 'hardware' AND t.created_at >= $2 AND t.created_at < $1) AS "cat_hardware!",
                 COUNT(*) FILTER (WHERE t.category = 'software' AND t.created_at >= $2 AND t.created_at < $1) AS "cat_software!",
                 COUNT(*) FILTER (WHERE t.category = 'access'   AND t.created_at >= $2 AND t.created_at < $1) AS "cat_access!",
                 COUNT(*) FILTER (WHERE t.category = 'other'    AND t.created_at >= $2 AND t.created_at < $1) AS "cat_other!",
                 COUNT(*) FILTER (WHERE t.resolved_at >= $2 AND t.resolved_at < $1) AS "resolved_in_period!",
                 (AVG(EXTRACT(EPOCH FROM (t.resolved_at - t.created_at)))
                    FILTER (WHERE t.resolved_at >= $2 AND t.resolved_at < $1))::float8 AS "avg_resolve_secs?"
               FROM ticket.tickets t
               WHERE t.created_at < $1::timestamptz"#,
            period.end,
            period.start,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;

        let by_status = vec![
            (TicketStatus::Open, count(r.open)),
            (TicketStatus::Triaged, count(r.triaged)),
            (TicketStatus::Assigned, count(r.assigned)),
            (TicketStatus::InProgress, count(r.in_progress)),
            (TicketStatus::Resolved, count(r.resolved)),
            (TicketStatus::Closed, count(r.closed)),
            (TicketStatus::Reopened, count(r.reopened)),
        ];
        let by_category = vec![
            (TicketCategory::Hardware, count(r.cat_hardware)),
            (TicketCategory::Software, count(r.cat_software)),
            (TicketCategory::Access, count(r.cat_access)),
            (TicketCategory::Other, count(r.cat_other)),
        ];

        Ok(TicketStats {
            created_in_period: count(r.created_in_period),
            resolved_in_period: count(r.resolved_in_period),
            by_status,
            by_category,
            avg_resolve_hours: r.avg_resolve_secs.map(|s| s / 3600.0),
        })
    }

    #[tracing::instrument(skip_all)]
    async fn staff_stats_by_group(
        &self,
        period: Period,
    ) -> Result<Vec<GroupStaffStats>, RepositoryError> {
        let rows = sqlx::query_as!(
            StaffStatsRow,
            r#"SELECT
                 g.id AS "group_id!",
                 COUNT(m.id) FILTER (
                   WHERE m.joined_at < $1
                     AND (m.deactivated_at IS NULL OR m.deactivated_at >= $1)
                 )                                                                AS "headcount!",
                 COUNT(m.id) FILTER (WHERE m.joined_at      >= $2 AND m.joined_at      < $1) AS "new_joiners!",
                 COUNT(m.id) FILTER (WHERE m.deactivated_at >= $2 AND m.deactivated_at < $1) AS "deactivations!"
               FROM org.groups g
               LEFT JOIN org.memberships m ON m.group_id = g.id
               GROUP BY g.id
               ORDER BY g.id"#,
            period.end,
            period.start,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;

        Ok(rows
            .into_iter()
            .map(|r| GroupStaffStats {
                group_id: GroupId(r.group_id),
                headcount: count(r.headcount),
                new_joiners: count(r.new_joiners),
                deactivations: count(r.deactivations),
            })
            .collect())
    }

    #[tracing::instrument(skip_all)]
    async fn company_staff_stats(
        &self,
        period: Period,
    ) -> Result<CompanyStaffStats, RepositoryError> {
        let r = sqlx::query!(
            r#"SELECT
                 COUNT(*) FILTER (
                   WHERE u.created_at < $1
                     AND (u.deactivated_at IS NULL OR u.deactivated_at >= $1)
                     AND u.status <> 'pending'
                 )                                                                       AS "active_users!",
                 COUNT(*) FILTER (WHERE u.first_logged_in_at >= $2 AND u.first_logged_in_at < $1) AS "new_active_users!",
                 COUNT(*) FILTER (WHERE u.deactivated_at     >= $2 AND u.deactivated_at     < $1) AS "deactivated_users!"
               FROM auth.users u"#,
            period.end,
            period.start,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;

        Ok(CompanyStaffStats {
            active_users: count(r.active_users),
            new_active_users: count(r.new_active_users),
            deactivated_users: count(r.deactivated_users),
        })
    }

    #[tracing::instrument(skip_all)]
    async fn monthly_growth(&self, year: i32) -> Result<Vec<MonthlyBucket>, RepositoryError> {
        let year_start = Date::from_calendar_date(year, Month::January, 1)
            .map_err(|e| RepositoryError::Backend(e.to_string()))?
            .midnight()
            .assume_utc();
        let next_year_start = Date::from_calendar_date(year + 1, Month::January, 1)
            .map_err(|e| RepositoryError::Backend(e.to_string()))?
            .midnight()
            .assume_utc();

        let rows = sqlx::query_as!(
            MonthlyBucketRow,
            r#"WITH months AS (
                 SELECT generate_series($1::timestamptz,
                                        $1::timestamptz + interval '11 months',
                                        interval '1 month') AS month
               ),
               joiners AS (
                 SELECT date_trunc('month', joined_at) AS month, COUNT(*) AS n
                 FROM org.memberships WHERE joined_at >= $1 AND joined_at < $2 GROUP BY 1
               ),
               deacts AS (
                 SELECT date_trunc('month', deactivated_at) AS month, COUNT(*) AS n
                 FROM org.memberships WHERE deactivated_at >= $1 AND deactivated_at < $2 GROUP BY 1
               ),
               tix AS (
                 SELECT date_trunc('month', created_at) AS month, COUNT(*) AS n
                 FROM ticket.tickets WHERE created_at >= $1 AND created_at < $2 GROUP BY 1
               ),
               proj_done AS (
                 SELECT date_trunc('month', completed_at) AS month, COUNT(*) AS n
                 FROM project.projects WHERE completed_at >= $1 AND completed_at < $2 GROUP BY 1
               ),
               req_done AS (
                 SELECT date_trunc('month', completed_at) AS month, COUNT(*) AS n
                 FROM project.requests WHERE completed_at >= $1 AND completed_at < $2 GROUP BY 1
               )
               SELECT
                 m.month                                  AS "month!",
                 COALESCE(j.n, 0)                         AS "new_joiners!",
                 COALESCE(d.n, 0)                         AS "deactivations!",
                 (SUM(COALESCE(j.n,0) - COALESCE(d.n,0))
                    OVER (ORDER BY m.month))::bigint       AS "headcount_delta_cum!",
                 COALESCE(x.n, 0)                         AS "tickets_created!",
                 COALESCE(pd.n, 0)                        AS "projects_completed!",
                 COALESCE(rd.n, 0)                        AS "requests_completed!"
               FROM months m
               LEFT JOIN joiners   j  ON j.month  = m.month
               LEFT JOIN deacts    d  ON d.month  = m.month
               LEFT JOIN tix       x  ON x.month  = m.month
               LEFT JOIN proj_done pd ON pd.month = m.month
               LEFT JOIN req_done  rd ON rd.month = m.month
               ORDER BY m.month"#,
            year_start,
            next_year_start,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;

        Ok(rows
            .into_iter()
            .map(|r| MonthlyBucket {
                year: r.month.year(),
                month: u8::from(r.month.month()),
                new_joiners: count(r.new_joiners),
                deactivations: count(r.deactivations),
                headcount_delta_cum: i32::try_from(r.headcount_delta_cum).unwrap_or(0),
                tickets_created: count(r.tickets_created),
                projects_completed: count(r.projects_completed),
                requests_completed: count(r.requests_completed),
            })
            .collect())
    }

    #[tracing::instrument(skip_all, fields(user = ?user))]
    async fn staff_monthly_stats(
        &self,
        user: UserId,
        period: Period,
    ) -> Result<StaffMonthlyStats, RepositoryError> {
        // Inclusive first/last day of the report month, for the DATE columns.
        let first = period.start.date();
        let last = period
            .end
            .date()
            .previous_day()
            .ok_or_else(|| RepositoryError::Backend("invalid report period".into()))?;

        // Approved daily-report hours by kind + distinct reported dates.
        let reports = sqlx::query!(
            r#"SELECT
                 COALESCE(SUM(e.hours) FILTER (WHERE e.kind = 'request_work'), 0)::float8 AS "request_work!",
                 COALESCE(SUM(e.hours) FILTER (WHERE e.kind = 'learning'), 0)::float8      AS "learning!",
                 COALESCE(SUM(e.hours) FILTER (WHERE e.kind = 'other'), 0)::float8         AS "other!",
                 COUNT(DISTINCT r.report_date)                                             AS "days_reported!"
               FROM attendance.daily_reports r
               LEFT JOIN attendance.daily_report_entries e ON e.daily_report_id = r.id
               WHERE r.user_id = $1
                 AND r.status = 'approved'
                 AND r.report_date >= $2
                 AND r.report_date <= $3"#,
            user.0,
            first,
            last,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;

        // Approved leave days by kind, over requests intersecting the month (same
        // overlap rule the work-percentage denominator uses). Approximation: a
        // request spanning two months is counted in full in each overlapping month.
        let leave_rows = sqlx::query!(
            r#"SELECT
                 kind AS "kind: SqlDayOffKind",
                 COALESCE(SUM(days), 0)::float8 AS "days!"
               FROM attendance.dayoff
               WHERE requester_user_id = $1
                 AND status = 'approved'
                 AND start_date <= $3
                 AND end_date >= $2
               GROUP BY kind"#,
            user.0,
            first,
            last,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        let leave_days_by_kind: Vec<(DayOffKind, f64)> = leave_rows
            .into_iter()
            .map(|r| (DayOffKind::from(r.kind), r.days))
            .collect();

        // Approved overtime hours worked in the month.
        let overtime = sqlx::query!(
            r#"SELECT COALESCE(SUM(hours), 0)::float8 AS "hours!"
               FROM attendance.overtime
               WHERE requester_user_id = $1
                 AND status = 'approved'
                 AND work_date >= $2
                 AND work_date <= $3"#,
            user.0,
            first,
            last,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;

        // Approved flex day count in the month.
        let flex = sqlx::query!(
            r#"SELECT COUNT(*) AS "count!"
               FROM attendance.flex_hours
               WHERE user_id = $1
                 AND status = 'approved'
                 AND work_date >= $2
                 AND work_date <= $3"#,
            user.0,
            first,
            last,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;

        // Remaining balance on grants expiring within the report month.
        let balance = sqlx::query!(
            r#"SELECT COALESCE(SUM(days_remaining), 0)::float8 AS "soon!"
               FROM attendance.leave_grants
               WHERE user_id = $1
                 AND days_remaining > 0
                 AND expires_on >= $2
                 AND expires_on <= $3"#,
            user.0,
            first,
            last,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;

        // Requests assigned to the user: completed in the period vs still open,
        // plus the average progress over active (non-terminal) requests.
        let requests = sqlx::query!(
            r#"SELECT
                 COUNT(*) FILTER (
                   WHERE status = 'completed' AND completed_at >= $2 AND completed_at < $3
                 )                                                                          AS "completed!",
                 COUNT(*) FILTER (
                   WHERE status IN ('draft','submitted','assigned','in_progress','review')
                 )                                                                          AS "open!",
                 COALESCE(
                   AVG(progress) FILTER (WHERE status NOT IN ('completed','cancelled')), 0
                 )::float8                                                                  AS "avg_progress!"
               FROM project.requests
               WHERE assignee_user_id = $1"#,
            user.0,
            period.start,
            period.end,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;

        Ok(StaffMonthlyStats {
            days_reported: count(reports.days_reported),
            hours_request_work: reports.request_work,
            hours_learning: reports.learning,
            hours_other: reports.other,
            leave_days_by_kind,
            overtime_hours: overtime.hours,
            flex_days: count(flex.count),
            balance_expiring_soon: balance.soon,
            requests_completed: count(requests.completed),
            requests_open: count(requests.open),
            avg_request_progress: pct(requests.avg_progress),
        })
    }
}

#[async_trait]
impl ReportArchiveRepository for PgReportingRepo {
    #[tracing::instrument(skip_all)]
    async fn insert(&self, report: &Report) -> Result<(), RepositoryError> {
        let kind = SqlReportKind::from(report.kind);
        let scope = SqlReportScope::from(report.scope);
        let size_bytes = i64::try_from(report.size_bytes)
            .map_err(|_| RepositoryError::Backend("size_bytes exceeds i64::MAX".into()))?;
        sqlx::query!(
            r#"INSERT INTO reporting.reports
                 (id, kind, scope, group_id, period_start, period_end,
                  storage_key, content_type, size_bytes, generated_by, generated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
               ON CONFLICT DO NOTHING"#,
            report.id.0,
            kind as SqlReportKind,
            scope as SqlReportScope,
            report.group_id.map(|g| g.0),
            report.period_start,
            report.period_end,
            report.storage_key,
            report.content_type,
            size_bytes,
            report.generated_by.map(|u| u.0),
            report.generated_at,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(limit = ?limit))]
    async fn list(&self, limit: u32) -> Result<Vec<Report>, RepositoryError> {
        let rows = sqlx::query_as!(
            ReportRow,
            r#"SELECT
                 id,
                 kind        AS "kind: SqlReportKind",
                 scope       AS "scope: SqlReportScope",
                 group_id,
                 period_start,
                 period_end,
                 storage_key,
                 content_type,
                 size_bytes,
                 generated_by,
                 generated_at
               FROM reporting.reports
               ORDER BY generated_at DESC
               LIMIT $1"#,
            i64::from(limit),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        rows.into_iter().map(Report::try_from).collect()
    }

    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn find_by_id(&self, id: ReportId) -> Result<Option<Report>, RepositoryError> {
        let row = sqlx::query_as!(
            ReportRow,
            r#"SELECT
                 id,
                 kind        AS "kind: SqlReportKind",
                 scope       AS "scope: SqlReportScope",
                 group_id,
                 period_start,
                 period_end,
                 storage_key,
                 content_type,
                 size_bytes,
                 generated_by,
                 generated_at
               FROM reporting.reports
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        row.map(Report::try_from).transpose()
    }

    #[tracing::instrument(skip_all)]
    async fn find_by_period(
        &self,
        kind: ReportKind,
        period_start: OffsetDateTime,
    ) -> Result<Option<Report>, RepositoryError> {
        let kind = SqlReportKind::from(kind);
        let row = sqlx::query_as!(
            ReportRow,
            r#"SELECT
                 id,
                 kind        AS "kind: SqlReportKind",
                 scope       AS "scope: SqlReportScope",
                 group_id,
                 period_start,
                 period_end,
                 storage_key,
                 content_type,
                 size_bytes,
                 generated_by,
                 generated_at
               FROM reporting.reports
               WHERE kind = $1 AND scope = 'company' AND period_start = $2"#,
            kind as SqlReportKind,
            period_start,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        row.map(Report::try_from).transpose()
    }

    #[tracing::instrument(skip_all)]
    async fn list_all_storage_keys(&self) -> Result<Vec<String>, RepositoryError> {
        let rows = sqlx::query!(r#"SELECT storage_key FROM reporting.reports"#)
            .fetch_all(&self.pool)
            .await
            .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(|r| r.storage_key).collect())
    }
}
