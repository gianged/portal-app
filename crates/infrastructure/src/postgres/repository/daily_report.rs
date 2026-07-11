use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::PgPool;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{DailyReportEntryId, DailyReportId, GroupId, RequestId, UserId},
    model::{DailyReport, DailyReportEntry},
    repository::DailyReportRepository,
};

use crate::postgres::{
    enums::{SqlDailyReportEntryKind, SqlDailyReportStatus},
    mappers,
};

pub struct PgDailyReportRepo {
    pool: PgPool,
}

impl PgDailyReportRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Loads every entry belonging to the given report ids in one query.
    async fn entries_for(&self, report_ids: &[Uuid]) -> Result<Vec<EntryRow>, RepositoryError> {
        sqlx::query_as!(
            EntryRow,
            r#"SELECT
                 id,
                 daily_report_id,
                 kind        AS "kind: SqlDailyReportEntryKind",
                 description,
                 request_id,
                 hours::float8 AS "hours?",
                 created_at
               FROM attendance.daily_report_entries
               WHERE daily_report_id = ANY($1)
               ORDER BY created_at"#,
            report_ids,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
    }

    /// Attaches each report's entries (one batched lookup) and assembles aggregates.
    async fn hydrate(&self, reports: Vec<ReportRow>) -> Result<Vec<DailyReport>, RepositoryError> {
        if reports.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = reports.iter().map(|r| r.id).collect();
        let mut grouped: HashMap<Uuid, Vec<DailyReportEntry>> = HashMap::new();
        for e in self.entries_for(&ids).await? {
            grouped.entry(e.daily_report_id).or_default().push(e.into());
        }
        Ok(reports
            .into_iter()
            .map(|r| {
                let entries = grouped.remove(&r.id).unwrap_or_default();
                report_from_row(r, entries)
            })
            .collect())
    }
}

// NUMERIC `hours` is read as float8 (no decimal feature) into Option<f64>.
struct ReportRow {
    id: Uuid,
    user_id: Uuid,
    report_date: Date,
    status: SqlDailyReportStatus,
    summary: String,
    submitted_at: Option<OffsetDateTime>,
    reviewed_by_user_id: Option<Uuid>,
    reviewed_at: Option<OffsetDateTime>,
    review_note: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

struct EntryRow {
    id: Uuid,
    daily_report_id: Uuid,
    kind: SqlDailyReportEntryKind,
    description: String,
    request_id: Option<Uuid>,
    hours: Option<f64>,
    created_at: OffsetDateTime,
}

impl From<EntryRow> for DailyReportEntry {
    fn from(r: EntryRow) -> Self {
        Self {
            id: DailyReportEntryId(r.id),
            daily_report_id: DailyReportId(r.daily_report_id),
            kind: r.kind.into(),
            description: r.description,
            request_id: r.request_id.map(RequestId),
            hours: r.hours,
            created_at: r.created_at,
        }
    }
}

fn report_from_row(r: ReportRow, entries: Vec<DailyReportEntry>) -> DailyReport {
    DailyReport {
        id: DailyReportId(r.id),
        user_id: UserId(r.user_id),
        report_date: r.report_date,
        status: r.status.into(),
        summary: r.summary,
        entries,
        submitted_at: r.submitted_at,
        reviewed_by: r.reviewed_by_user_id.map(UserId),
        reviewed_at: r.reviewed_at,
        review_note: r.review_note,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

#[async_trait]
impl DailyReportRepository for PgDailyReportRepo {
    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn find_by_id(&self, id: DailyReportId) -> Result<Option<DailyReport>, RepositoryError> {
        let row = sqlx::query_as!(
            ReportRow,
            r#"SELECT
                 id,
                 user_id,
                 report_date,
                 status AS "status: SqlDailyReportStatus",
                 summary,
                 submitted_at,
                 reviewed_by_user_id,
                 reviewed_at,
                 review_note,
                 created_at,
                 updated_at
               FROM attendance.daily_reports
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(self.hydrate(row.into_iter().collect()).await?.pop())
    }

    #[tracing::instrument(skip_all, fields(user = ?user, date = ?date))]
    async fn find_by_user_date(
        &self,
        user: UserId,
        date: Date,
    ) -> Result<Option<DailyReport>, RepositoryError> {
        let row = sqlx::query_as!(
            ReportRow,
            r#"SELECT
                 id,
                 user_id,
                 report_date,
                 status AS "status: SqlDailyReportStatus",
                 summary,
                 submitted_at,
                 reviewed_by_user_id,
                 reviewed_at,
                 review_note,
                 created_at,
                 updated_at
               FROM attendance.daily_reports
               WHERE user_id = $1 AND report_date = $2"#,
            user.0,
            date,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(self.hydrate(row.into_iter().collect()).await?.pop())
    }

    #[tracing::instrument(skip_all, fields(user = ?user))]
    async fn list_for_user(
        &self,
        user: UserId,
        from: Date,
        to: Date,
    ) -> Result<Vec<DailyReport>, RepositoryError> {
        let rows = sqlx::query_as!(
            ReportRow,
            r#"SELECT
                 id,
                 user_id,
                 report_date,
                 status AS "status: SqlDailyReportStatus",
                 summary,
                 submitted_at,
                 reviewed_by_user_id,
                 reviewed_at,
                 review_note,
                 created_at,
                 updated_at
               FROM attendance.daily_reports
               WHERE user_id = $1 AND report_date BETWEEN $2 AND $3
               ORDER BY report_date DESC"#,
            user.0,
            from,
            to,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        self.hydrate(rows).await
    }

    #[tracing::instrument(skip_all, fields(group = ?group))]
    async fn list_for_group(
        &self,
        group: GroupId,
        from: Date,
        to: Date,
    ) -> Result<Vec<DailyReport>, RepositoryError> {
        let rows = sqlx::query_as!(
            ReportRow,
            r#"SELECT
                 r.id,
                 r.user_id,
                 r.report_date,
                 r.status AS "status: SqlDailyReportStatus",
                 r.summary,
                 r.submitted_at,
                 r.reviewed_by_user_id,
                 r.reviewed_at,
                 r.review_note,
                 r.created_at,
                 r.updated_at
               FROM attendance.daily_reports r
               JOIN org.memberships m ON m.user_id = r.user_id
               WHERE m.group_id = $1
                 AND m.deactivated_at IS NULL
                 AND r.report_date BETWEEN $2 AND $3
               ORDER BY r.report_date DESC, r.user_id"#,
            group.0,
            from,
            to,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        self.hydrate(rows).await
    }

    #[tracing::instrument(skip_all)]
    async fn save(&self, report: &DailyReport) -> Result<(), RepositoryError> {
        let status = SqlDailyReportStatus::from(report.status);
        let mut tx = self.pool.begin().await.map_err(mappers::map_pg_error)?;

        sqlx::query!(
            r#"INSERT INTO attendance.daily_reports
                 (id, user_id, report_date, status, summary, submitted_at,
                  reviewed_by_user_id, reviewed_at, review_note, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
               ON CONFLICT (id) DO UPDATE SET
                 user_id             = EXCLUDED.user_id,
                 report_date         = EXCLUDED.report_date,
                 status              = EXCLUDED.status,
                 summary             = EXCLUDED.summary,
                 submitted_at        = EXCLUDED.submitted_at,
                 reviewed_by_user_id = EXCLUDED.reviewed_by_user_id,
                 reviewed_at         = EXCLUDED.reviewed_at,
                 review_note         = EXCLUDED.review_note"#,
            report.id.0,
            report.user_id.0,
            report.report_date,
            status as SqlDailyReportStatus,
            report.summary,
            report.submitted_at,
            report.reviewed_by.map(|u| u.0),
            report.reviewed_at,
            report.review_note,
            report.created_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(mappers::map_pg_error)?;

        sqlx::query!(
            r#"DELETE FROM attendance.daily_report_entries WHERE daily_report_id = $1"#,
            report.id.0,
        )
        .execute(&mut *tx)
        .await
        .map_err(mappers::map_pg_error)?;

        if !report.entries.is_empty() {
            let ids: Vec<Uuid> = report.entries.iter().map(|e| e.id.0).collect();
            let report_ids: Vec<Uuid> =
                report.entries.iter().map(|e| e.daily_report_id.0).collect();
            let kinds: Vec<SqlDailyReportEntryKind> =
                report.entries.iter().map(|e| e.kind.into()).collect();
            let descriptions: Vec<String> = report
                .entries
                .iter()
                .map(|e| e.description.clone())
                .collect();
            let request_ids: Vec<Option<Uuid>> = report
                .entries
                .iter()
                .map(|e| e.request_id.map(|r| r.0))
                .collect();
            let hours: Vec<Option<f64>> = report.entries.iter().map(|e| e.hours).collect();
            let created: Vec<OffsetDateTime> =
                report.entries.iter().map(|e| e.created_at).collect();
            sqlx::query!(
                r#"INSERT INTO attendance.daily_report_entries
                     (id, daily_report_id, kind, description, request_id, hours, created_at)
                   SELECT u.id, u.daily_report_id, u.kind, u.description, u.request_id,
                          u.hours::numeric, u.created_at
                   FROM UNNEST($1::uuid[], $2::uuid[], $3::attendance.daily_report_entry_kind[],
                               $4::text[], $5::uuid[], $6::float8[], $7::timestamptz[])
                     AS u(id, daily_report_id, kind, description, request_id, hours, created_at)"#,
                &ids,
                &report_ids,
                kinds as Vec<SqlDailyReportEntryKind>,
                &descriptions,
                request_ids as Vec<Option<Uuid>>,
                hours as Vec<Option<f64>>,
                &created,
            )
            .execute(&mut *tx)
            .await
            .map_err(mappers::map_pg_error)?;
        }

        tx.commit().await.map_err(mappers::map_pg_error)?;
        Ok(())
    }
}
