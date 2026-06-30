use async_trait::async_trait;
use sqlx::PgPool;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{GroupId, OvertimeId, UserId},
    model::Overtime,
    repository::OvertimeRepository,
};

use crate::postgres::{enums::SqlOvertimeStatus, mappers};

pub struct PgOvertimeRepo {
    pool: PgPool,
}

impl PgOvertimeRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// NUMERIC `hours` is read via ::float8 (no decimal feature).
struct OvertimeRow {
    id: Uuid,
    requester_user_id: Uuid,
    work_date: Date,
    hours: f64,
    reason: String,
    status: SqlOvertimeStatus,
    leader_user_id: Option<Uuid>,
    leader_decided_at: Option<OffsetDateTime>,
    hr_user_id: Option<Uuid>,
    hr_decided_at: Option<OffsetDateTime>,
    decision_note: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl From<OvertimeRow> for Overtime {
    fn from(r: OvertimeRow) -> Self {
        Self {
            id: OvertimeId(r.id),
            requester_user_id: UserId(r.requester_user_id),
            work_date: r.work_date,
            hours: r.hours,
            reason: r.reason,
            status: r.status.into(),
            leader_user_id: r.leader_user_id.map(UserId),
            leader_decided_at: r.leader_decided_at,
            hr_user_id: r.hr_user_id.map(UserId),
            hr_decided_at: r.hr_decided_at,
            decision_note: r.decision_note,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[async_trait]
impl OvertimeRepository for PgOvertimeRepo {
    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn find_by_id(&self, id: OvertimeId) -> Result<Option<Overtime>, RepositoryError> {
        let row = sqlx::query_as!(
            OvertimeRow,
            r#"SELECT
                 id, requester_user_id, work_date,
                 hours::float8 AS "hours!",
                 reason,
                 status AS "status: SqlOvertimeStatus",
                 leader_user_id, leader_decided_at, hr_user_id, hr_decided_at,
                 decision_note, created_at, updated_at
               FROM attendance.overtime
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(row.map(Into::into))
    }

    #[tracing::instrument(skip_all, fields(user = ?user))]
    async fn list_for_user(
        &self,
        user: UserId,
        from: Date,
        to: Date,
    ) -> Result<Vec<Overtime>, RepositoryError> {
        let rows = sqlx::query_as!(
            OvertimeRow,
            r#"SELECT
                 id, requester_user_id, work_date,
                 hours::float8 AS "hours!",
                 reason,
                 status AS "status: SqlOvertimeStatus",
                 leader_user_id, leader_decided_at, hr_user_id, hr_decided_at,
                 decision_note, created_at, updated_at
               FROM attendance.overtime
               WHERE requester_user_id = $1 AND work_date >= $2 AND work_date <= $3
               ORDER BY work_date DESC"#,
            user.0,
            from,
            to,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(user = ?user, year, month))]
    async fn approved_hours_in_month(
        &self,
        user: UserId,
        year: i32,
        month: u32,
    ) -> Result<f64, RepositoryError> {
        let (first, last) = mappers::month_bounds(year, month)?;
        let row = sqlx::query!(
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
        Ok(row.hours)
    }

    #[tracing::instrument(skip_all, fields(group = ?group))]
    async fn list_pending_for_leader(
        &self,
        group: GroupId,
    ) -> Result<Vec<Overtime>, RepositoryError> {
        let rows = sqlx::query_as!(
            OvertimeRow,
            r#"SELECT
                 o.id, o.requester_user_id, o.work_date,
                 o.hours::float8 AS "hours!",
                 o.reason,
                 o.status AS "status: SqlOvertimeStatus",
                 o.leader_user_id, o.leader_decided_at, o.hr_user_id, o.hr_decided_at,
                 o.decision_note, o.created_at, o.updated_at
               FROM attendance.overtime o
               JOIN org.memberships mem ON mem.user_id = o.requester_user_id
               WHERE mem.group_id = $1
                 AND mem.deactivated_at IS NULL
                 AND o.status = 'pending'
               ORDER BY o.created_at"#,
            group.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn list_pending_for_hr(&self) -> Result<Vec<Overtime>, RepositoryError> {
        let rows = sqlx::query_as!(
            OvertimeRow,
            r#"SELECT
                 id, requester_user_id, work_date,
                 hours::float8 AS "hours!",
                 reason,
                 status AS "status: SqlOvertimeStatus",
                 leader_user_id, leader_decided_at, hr_user_id, hr_decided_at,
                 decision_note, created_at, updated_at
               FROM attendance.overtime
               WHERE status = 'leader_approved'
               ORDER BY created_at"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(id = ?overtime.id))]
    async fn save(&self, overtime: &Overtime) -> Result<(), RepositoryError> {
        let status = SqlOvertimeStatus::from(overtime.status);
        sqlx::query!(
            r#"INSERT INTO attendance.overtime
                 (id, requester_user_id, work_date, hours, reason, status,
                  leader_user_id, leader_decided_at, hr_user_id, hr_decided_at,
                  decision_note, created_at, updated_at)
               VALUES ($1, $2, $3, $4::float8::numeric, $5, $6, $7, $8, $9, $10, $11, $12, $13)
               ON CONFLICT (id) DO UPDATE SET
                 work_date         = EXCLUDED.work_date,
                 hours             = EXCLUDED.hours,
                 reason            = EXCLUDED.reason,
                 status            = EXCLUDED.status,
                 leader_user_id    = EXCLUDED.leader_user_id,
                 leader_decided_at = EXCLUDED.leader_decided_at,
                 hr_user_id        = EXCLUDED.hr_user_id,
                 hr_decided_at     = EXCLUDED.hr_decided_at,
                 decision_note     = EXCLUDED.decision_note,
                 updated_at        = EXCLUDED.updated_at"#,
            overtime.id.0,
            overtime.requester_user_id.0,
            overtime.work_date,
            overtime.hours,
            overtime.reason,
            status as SqlOvertimeStatus,
            overtime.leader_user_id.map(|u| u.0),
            overtime.leader_decided_at,
            overtime.hr_user_id.map(|u| u.0),
            overtime.hr_decided_at,
            overtime.decision_note,
            overtime.created_at,
            overtime.updated_at,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }
}
