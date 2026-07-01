use async_trait::async_trait;
use sqlx::PgPool;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{DayOffId, GroupId, UserId},
    model::DayOff,
    repository::DayOffRepository,
};

use crate::postgres::{
    enums::{SqlDayOffKind, SqlDayOffStatus},
    mappers,
};

pub struct PgDayOffRepo {
    pool: PgPool,
}

impl PgDayOffRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// NUMERIC `days` is read via ::float8 (no decimal feature).
struct DayOffRow {
    id: Uuid,
    requester_user_id: Uuid,
    kind: SqlDayOffKind,
    start_date: Date,
    end_date: Date,
    start_half: bool,
    end_half: bool,
    days: f64,
    reason: String,
    status: SqlDayOffStatus,
    leader_user_id: Option<Uuid>,
    leader_decided_at: Option<OffsetDateTime>,
    hr_user_id: Option<Uuid>,
    hr_decided_at: Option<OffsetDateTime>,
    decision_note: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl From<DayOffRow> for DayOff {
    fn from(r: DayOffRow) -> Self {
        Self {
            id: DayOffId(r.id),
            requester_user_id: UserId(r.requester_user_id),
            kind: r.kind.into(),
            start_date: r.start_date,
            end_date: r.end_date,
            start_half: r.start_half,
            end_half: r.end_half,
            days: r.days,
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
impl DayOffRepository for PgDayOffRepo {
    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn find_by_id(&self, id: DayOffId) -> Result<Option<DayOff>, RepositoryError> {
        let row = sqlx::query_as!(
            DayOffRow,
            r#"SELECT
                 id, requester_user_id,
                 kind AS "kind: SqlDayOffKind",
                 start_date, end_date, start_half, end_half,
                 days::float8 AS "days!",
                 reason,
                 status AS "status: SqlDayOffStatus",
                 leader_user_id, leader_decided_at, hr_user_id, hr_decided_at,
                 decision_note, created_at, updated_at
               FROM attendance.dayoff
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
    ) -> Result<Vec<DayOff>, RepositoryError> {
        let rows = sqlx::query_as!(
            DayOffRow,
            r#"SELECT
                 id, requester_user_id,
                 kind AS "kind: SqlDayOffKind",
                 start_date, end_date, start_half, end_half,
                 days::float8 AS "days!",
                 reason,
                 status AS "status: SqlDayOffStatus",
                 leader_user_id, leader_decided_at, hr_user_id, hr_decided_at,
                 decision_note, created_at, updated_at
               FROM attendance.dayoff
               WHERE requester_user_id = $1 AND start_date <= $3 AND end_date >= $2
               ORDER BY start_date DESC"#,
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
    async fn approved_days_in_month(
        &self,
        user: UserId,
        year: i32,
        month: u32,
    ) -> Result<f64, RepositoryError> {
        let (first, last) = mappers::month_bounds(year, month)?;
        // Approximation: a request spanning two months counts in full in each overlapping month.
        let row = sqlx::query!(
            r#"SELECT COALESCE(SUM(days), 0)::float8 AS "days!"
               FROM attendance.dayoff
               WHERE requester_user_id = $1
                 AND status = 'approved'
                 AND start_date <= $3
                 AND end_date >= $2"#,
            user.0,
            first,
            last,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(row.days)
    }

    #[tracing::instrument(skip_all, fields(group = ?group))]
    async fn list_pending_for_leader(
        &self,
        group: GroupId,
    ) -> Result<Vec<DayOff>, RepositoryError> {
        let rows = sqlx::query_as!(
            DayOffRow,
            r#"SELECT
                 d.id, d.requester_user_id,
                 d.kind AS "kind: SqlDayOffKind",
                 d.start_date, d.end_date, d.start_half, d.end_half,
                 d.days::float8 AS "days!",
                 d.reason,
                 d.status AS "status: SqlDayOffStatus",
                 d.leader_user_id, d.leader_decided_at, d.hr_user_id, d.hr_decided_at,
                 d.decision_note, d.created_at, d.updated_at
               FROM attendance.dayoff d
               JOIN org.memberships mem ON mem.user_id = d.requester_user_id
               WHERE mem.group_id = $1
                 AND mem.deactivated_at IS NULL
                 AND d.status = 'pending'
               ORDER BY d.created_at"#,
            group.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn list_pending_for_hr(&self) -> Result<Vec<DayOff>, RepositoryError> {
        let rows = sqlx::query_as!(
            DayOffRow,
            r#"SELECT
                 id, requester_user_id,
                 kind AS "kind: SqlDayOffKind",
                 start_date, end_date, start_half, end_half,
                 days::float8 AS "days!",
                 reason,
                 status AS "status: SqlDayOffStatus",
                 leader_user_id, leader_decided_at, hr_user_id, hr_decided_at,
                 decision_note, created_at, updated_at
               FROM attendance.dayoff
               WHERE status = 'leader_approved' AND kind = 'annual_leave'
               ORDER BY created_at"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(id = ?day_off.id))]
    async fn save(&self, day_off: &DayOff) -> Result<(), RepositoryError> {
        let kind = SqlDayOffKind::from(day_off.kind);
        let status = SqlDayOffStatus::from(day_off.status);
        sqlx::query!(
            r#"INSERT INTO attendance.dayoff
                 (id, requester_user_id, kind, start_date, end_date, start_half, end_half,
                  days, reason, status, leader_user_id, leader_decided_at, hr_user_id,
                  hr_decided_at, decision_note, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8::float8::numeric, $9, $10, $11, $12,
                       $13, $14, $15, $16, $17)
               ON CONFLICT (id) DO UPDATE SET
                 kind              = EXCLUDED.kind,
                 start_date        = EXCLUDED.start_date,
                 end_date          = EXCLUDED.end_date,
                 start_half        = EXCLUDED.start_half,
                 end_half          = EXCLUDED.end_half,
                 days              = EXCLUDED.days,
                 reason            = EXCLUDED.reason,
                 status            = EXCLUDED.status,
                 leader_user_id    = EXCLUDED.leader_user_id,
                 leader_decided_at = EXCLUDED.leader_decided_at,
                 hr_user_id        = EXCLUDED.hr_user_id,
                 hr_decided_at     = EXCLUDED.hr_decided_at,
                 decision_note     = EXCLUDED.decision_note,
                 updated_at        = EXCLUDED.updated_at"#,
            day_off.id.0,
            day_off.requester_user_id.0,
            kind as SqlDayOffKind,
            day_off.start_date,
            day_off.end_date,
            day_off.start_half,
            day_off.end_half,
            day_off.days,
            day_off.reason,
            status as SqlDayOffStatus,
            day_off.leader_user_id.map(|u| u.0),
            day_off.leader_decided_at,
            day_off.hr_user_id.map(|u| u.0),
            day_off.hr_decided_at,
            day_off.decision_note,
            day_off.created_at,
            day_off.updated_at,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }
}
