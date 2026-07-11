use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::PgPool;
use time::{Date, OffsetDateTime, Time};
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{FlexHoursId, FlexSegmentId, GroupId, UserId},
    model::{FlexHours, FlexSegment},
    repository::FlexHoursRepository,
};

use crate::postgres::{enums::SqlFlexStatus, mappers};

pub struct PgFlexRepo {
    pool: PgPool,
}

impl PgFlexRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Loads every segment belonging to the given flex ids in one query.
    async fn segments_for(&self, flex_ids: &[Uuid]) -> Result<Vec<SegmentRow>, RepositoryError> {
        sqlx::query_as!(
            SegmentRow,
            r#"SELECT id, flex_id, seq, start_at, end_at
               FROM attendance.flex_segments
               WHERE flex_id = ANY($1)
               ORDER BY seq"#,
            flex_ids,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
    }

    /// Attaches each request's segments (one batched lookup).
    async fn hydrate(&self, rows: Vec<FlexRow>) -> Result<Vec<FlexHours>, RepositoryError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
        let mut grouped: HashMap<Uuid, Vec<FlexSegment>> = HashMap::new();
        for s in self.segments_for(&ids).await? {
            grouped.entry(s.flex_id).or_default().push(s.into());
        }
        Ok(rows
            .into_iter()
            .map(|r| {
                let segments = grouped.remove(&r.id).unwrap_or_default();
                flex_from_row(r, segments)
            })
            .collect())
    }
}

struct FlexRow {
    id: Uuid,
    user_id: Uuid,
    work_date: Date,
    status: SqlFlexStatus,
    leader_user_id: Option<Uuid>,
    decided_at: Option<OffsetDateTime>,
    decision_note: String,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

struct SegmentRow {
    id: Uuid,
    flex_id: Uuid,
    seq: i16,
    start_at: Time,
    end_at: Time,
}

impl From<SegmentRow> for FlexSegment {
    fn from(r: SegmentRow) -> Self {
        Self {
            id: FlexSegmentId(r.id),
            flex_id: FlexHoursId(r.flex_id),
            seq: u16::try_from(r.seq).unwrap_or(0),
            start: r.start_at,
            end: r.end_at,
        }
    }
}

fn flex_from_row(r: FlexRow, segments: Vec<FlexSegment>) -> FlexHours {
    FlexHours {
        id: FlexHoursId(r.id),
        user_id: UserId(r.user_id),
        work_date: r.work_date,
        segments,
        status: r.status.into(),
        leader_user_id: r.leader_user_id.map(UserId),
        decided_at: r.decided_at,
        decision_note: r.decision_note,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

#[async_trait]
impl FlexHoursRepository for PgFlexRepo {
    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn find_by_id(&self, id: FlexHoursId) -> Result<Option<FlexHours>, RepositoryError> {
        let row = sqlx::query_as!(
            FlexRow,
            r#"SELECT
                 id, user_id, work_date,
                 status AS "status: SqlFlexStatus",
                 leader_user_id, decided_at, decision_note, created_at, updated_at
               FROM attendance.flex_hours
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
    ) -> Result<Option<FlexHours>, RepositoryError> {
        let row = sqlx::query_as!(
            FlexRow,
            r#"SELECT
                 id, user_id, work_date,
                 status AS "status: SqlFlexStatus",
                 leader_user_id, decided_at, decision_note, created_at, updated_at
               FROM attendance.flex_hours
               WHERE user_id = $1 AND work_date = $2"#,
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
    ) -> Result<Vec<FlexHours>, RepositoryError> {
        let rows = sqlx::query_as!(
            FlexRow,
            r#"SELECT
                 id, user_id, work_date,
                 status AS "status: SqlFlexStatus",
                 leader_user_id, decided_at, decision_note, created_at, updated_at
               FROM attendance.flex_hours
               WHERE user_id = $1 AND work_date >= $2 AND work_date <= $3
               ORDER BY work_date DESC"#,
            user.0,
            from,
            to,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        self.hydrate(rows).await
    }

    #[tracing::instrument(skip_all, fields(user = ?user, year, month))]
    async fn approved_count_in_month(
        &self,
        user: UserId,
        year: i32,
        month: u8,
    ) -> Result<u32, RepositoryError> {
        let (first, last) = mappers::month_bounds(year, month)?;
        let row = sqlx::query!(
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
        Ok(u32::try_from(row.count).unwrap_or(0))
    }

    #[tracing::instrument(skip_all, fields(user = ?user, year, month))]
    async fn approved_hours_in_month(
        &self,
        user: UserId,
        year: i32,
        month: u8,
    ) -> Result<f64, RepositoryError> {
        let (first, last) = mappers::month_bounds(year, month)?;
        let row = sqlx::query!(
            r#"SELECT
                 COALESCE(SUM(EXTRACT(EPOCH FROM (s.end_at - s.start_at))), 0)::float8 / 3600.0
                   AS "hours!"
               FROM attendance.flex_segments s
               JOIN attendance.flex_hours f ON f.id = s.flex_id
               WHERE f.user_id = $1
                 AND f.status = 'approved'
                 AND f.work_date >= $2
                 AND f.work_date <= $3"#,
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
    ) -> Result<Vec<FlexHours>, RepositoryError> {
        let rows = sqlx::query_as!(
            FlexRow,
            r#"SELECT
                 f.id, f.user_id, f.work_date,
                 f.status AS "status: SqlFlexStatus",
                 f.leader_user_id, f.decided_at, f.decision_note, f.created_at, f.updated_at
               FROM attendance.flex_hours f
               JOIN org.memberships mem ON mem.user_id = f.user_id
               WHERE mem.group_id = $1
                 AND mem.deactivated_at IS NULL
                 AND f.status = 'pending'
               ORDER BY f.created_at"#,
            group.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        self.hydrate(rows).await
    }

    #[tracing::instrument(skip_all, fields(year, month))]
    async fn users_with_approved_flex_in_month(
        &self,
        year: i32,
        month: u8,
    ) -> Result<Vec<UserId>, RepositoryError> {
        let (first, last) = mappers::month_bounds(year, month)?;
        let rows = sqlx::query!(
            r#"SELECT DISTINCT user_id
               FROM attendance.flex_hours
               WHERE status = 'approved'
                 AND work_date >= $1
                 AND work_date <= $2"#,
            first,
            last,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(|r| UserId(r.user_id)).collect())
    }

    #[tracing::instrument(skip_all, fields(id = ?flex.id))]
    async fn save(&self, flex: &FlexHours) -> Result<(), RepositoryError> {
        let status = SqlFlexStatus::from(flex.status);
        let mut tx = self.pool.begin().await.map_err(mappers::map_pg_error)?;

        sqlx::query!(
            r#"INSERT INTO attendance.flex_hours
                 (id, user_id, work_date, status, leader_user_id, decided_at,
                  decision_note, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (id) DO UPDATE SET
                 work_date      = EXCLUDED.work_date,
                 status         = EXCLUDED.status,
                 leader_user_id = EXCLUDED.leader_user_id,
                 decided_at     = EXCLUDED.decided_at,
                 decision_note  = EXCLUDED.decision_note"#,
            flex.id.0,
            flex.user_id.0,
            flex.work_date,
            status as SqlFlexStatus,
            flex.leader_user_id.map(|u| u.0),
            flex.decided_at,
            flex.decision_note,
            flex.created_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(mappers::map_pg_error)?;

        sqlx::query!(
            r#"DELETE FROM attendance.flex_segments WHERE flex_id = $1"#,
            flex.id.0,
        )
        .execute(&mut *tx)
        .await
        .map_err(mappers::map_pg_error)?;

        if !flex.segments.is_empty() {
            let ids: Vec<Uuid> = flex.segments.iter().map(|s| s.id.0).collect();
            let flex_ids: Vec<Uuid> = flex.segments.iter().map(|s| s.flex_id.0).collect();
            let seqs: Vec<i16> = flex
                .segments
                .iter()
                .map(|s| {
                    i16::try_from(s.seq)
                        .map_err(|_| RepositoryError::Backend("seq out of range".into()))
                })
                .collect::<Result<_, _>>()?;
            let starts: Vec<Time> = flex.segments.iter().map(|s| s.start).collect();
            let ends: Vec<Time> = flex.segments.iter().map(|s| s.end).collect();
            sqlx::query!(
                r#"INSERT INTO attendance.flex_segments (id, flex_id, seq, start_at, end_at)
                   SELECT u.id, u.flex_id, u.seq, u.start_at, u.end_at
                   FROM UNNEST($1::uuid[], $2::uuid[], $3::int2[], $4::time[], $5::time[])
                     AS u(id, flex_id, seq, start_at, end_at)"#,
                &ids,
                &flex_ids,
                &seqs,
                &starts,
                &ends,
            )
            .execute(&mut *tx)
            .await
            .map_err(mappers::map_pg_error)?;
        }

        tx.commit().await.map_err(mappers::map_pg_error)?;
        Ok(())
    }
}
