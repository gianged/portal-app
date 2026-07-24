use async_trait::async_trait;
use domain::{
    error::RepositoryError,
    ids::{DayOffId, LeaveGrantId, LeaveTransactionId, UserId},
    model::{LeaveGrant, LeaveTransaction},
    repository::LeaveBalanceRepository,
};
use sqlx::PgPool;
use time::{Date, Duration, OffsetDateTime};
use uuid::Uuid;

use crate::postgres::{enums::SqlLeaveTxnKind, mappers};

pub struct PgLeaveBalanceRepo {
    pool: PgPool,
}

impl PgLeaveBalanceRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// NUMERIC columns are read/written via ::float8 (no decimal feature).
struct GrantRow {
    id: Uuid,
    user_id: Uuid,
    grant_year: i16,
    days_granted: f64,
    days_remaining: f64,
    expires_on: Date,
    created_by_user_id: Option<Uuid>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl From<GrantRow> for LeaveGrant {
    fn from(r: GrantRow) -> Self {
        Self {
            id: LeaveGrantId(r.id),
            user_id: UserId(r.user_id),
            grant_year: u16::try_from(r.grant_year).unwrap_or(0),
            days_granted: r.days_granted,
            days_remaining: r.days_remaining,
            expires_on: r.expires_on,
            created_by: r.created_by_user_id.map(UserId),
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

struct TxnRow {
    id: Uuid,
    user_id: Uuid,
    grant_id: Uuid,
    kind: SqlLeaveTxnKind,
    delta: f64,
    dayoff_id: Option<Uuid>,
    work_pct: Option<f64>,
    reason: String,
    created_by_user_id: Option<Uuid>,
    created_at: OffsetDateTime,
}

impl From<TxnRow> for LeaveTransaction {
    fn from(r: TxnRow) -> Self {
        Self {
            id: LeaveTransactionId(r.id),
            user_id: UserId(r.user_id),
            grant_id: LeaveGrantId(r.grant_id),
            kind: r.kind.into(),
            delta: r.delta,
            dayoff_id: r.dayoff_id.map(DayOffId),
            work_pct: r.work_pct,
            reason: r.reason,
            created_by: r.created_by_user_id.map(UserId),
            created_at: r.created_at,
        }
    }
}

#[async_trait]
impl LeaveBalanceRepository for PgLeaveBalanceRepo {
    #[tracing::instrument(skip_all, fields(user = ?user))]
    async fn list_grants(&self, user: UserId) -> Result<Vec<LeaveGrant>, RepositoryError> {
        let rows = sqlx::query_as!(
            GrantRow,
            r#"SELECT
                 id,
                 user_id,
                 grant_year,
                 days_granted::float8   AS "days_granted!",
                 days_remaining::float8 AS "days_remaining!",
                 expires_on,
                 created_by_user_id,
                 created_at,
                 updated_at
               FROM attendance.leave_grants
               WHERE user_id = $1
               ORDER BY grant_year DESC"#,
            user.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(user = ?user, asof = ?asof))]
    async fn available(&self, user: UserId, asof: Date) -> Result<f64, RepositoryError> {
        let row = sqlx::query!(
            r#"SELECT COALESCE(SUM(days_remaining), 0)::float8 AS "available!"
               FROM attendance.leave_grants
               WHERE user_id = $1 AND expires_on > $2"#,
            user.0,
            asof,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(row.available)
    }

    #[tracing::instrument(skip_all, fields(grant = ?grant.id))]
    async fn upsert_grant_with_txn(
        &self,
        grant: &LeaveGrant,
        txn: Option<&LeaveTransaction>,
    ) -> Result<(), RepositoryError> {
        let grant_year = i16::try_from(grant.grant_year)
            .map_err(|_| RepositoryError::Backend("grant_year out of range".into()))?;
        let mut tx = self.pool.begin().await.map_err(mappers::map_pg_error)?;
        sqlx::query!(
            r#"INSERT INTO attendance.leave_grants
                 (id, user_id, grant_year, days_granted, days_remaining, expires_on,
                  created_by_user_id, created_at)
               VALUES ($1, $2, $3, $4::float8::numeric, $5::float8::numeric, $6, $7, $8)
               ON CONFLICT (id) DO UPDATE SET
                 days_granted   = EXCLUDED.days_granted,
                 days_remaining = EXCLUDED.days_remaining,
                 expires_on     = EXCLUDED.expires_on"#,
            grant.id.0,
            grant.user_id.0,
            grant_year,
            grant.days_granted,
            grant.days_remaining,
            grant.expires_on,
            grant.created_by.map(|u| u.0),
            grant.created_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(mappers::map_pg_error)?;
        if let Some(t) = txn {
            sqlx::query!(
                r#"INSERT INTO attendance.leave_transactions
                     (id, user_id, grant_id, kind, delta, dayoff_id, work_pct, reason,
                      created_by_user_id, created_at)
                   VALUES ($1, $2, $3, $4, $5::float8::numeric, $6, $7::float8::numeric, $8, $9, $10)"#,
                t.id.0,
                t.user_id.0,
                t.grant_id.0,
                SqlLeaveTxnKind::from(t.kind) as SqlLeaveTxnKind,
                t.delta,
                t.dayoff_id.map(|d| d.0),
                t.work_pct,
                t.reason,
                t.created_by.map(|u| u.0),
                t.created_at,
            )
            .execute(&mut *tx)
            .await
            .map_err(mappers::map_pg_error)?;
        }
        tx.commit().await.map_err(mappers::map_pg_error)?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn apply(
        &self,
        grant_deltas: &[(LeaveGrantId, f64)],
        txns: &[LeaveTransaction],
    ) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(mappers::map_pg_error)?;

        if !grant_deltas.is_empty() {
            let ids: Vec<Uuid> = grant_deltas.iter().map(|(g, _)| g.0).collect();
            let deltas: Vec<f64> = grant_deltas.iter().map(|(_, d)| *d).collect();
            sqlx::query!(
                r#"UPDATE attendance.leave_grants g
                   SET days_remaining = g.days_remaining + u.delta::numeric
                   FROM UNNEST($1::uuid[], $2::float8[]) AS u(id, delta)
                   WHERE g.id = u.id"#,
                &ids,
                &deltas,
            )
            .execute(&mut *tx)
            .await
            .map_err(mappers::map_pg_error)?;
        }

        if !txns.is_empty() {
            let ids: Vec<Uuid> = txns.iter().map(|t| t.id.0).collect();
            let user_ids: Vec<Uuid> = txns.iter().map(|t| t.user_id.0).collect();
            let grant_ids: Vec<Uuid> = txns.iter().map(|t| t.grant_id.0).collect();
            let kinds: Vec<SqlLeaveTxnKind> = txns.iter().map(|t| t.kind.into()).collect();
            let deltas: Vec<f64> = txns.iter().map(|t| t.delta).collect();
            let dayoff_ids: Vec<Option<Uuid>> =
                txns.iter().map(|t| t.dayoff_id.map(|d| d.0)).collect();
            let work_pcts: Vec<Option<f64>> = txns.iter().map(|t| t.work_pct).collect();
            let reasons: Vec<String> = txns.iter().map(|t| t.reason.clone()).collect();
            let created_bys: Vec<Option<Uuid>> =
                txns.iter().map(|t| t.created_by.map(|u| u.0)).collect();
            let created: Vec<OffsetDateTime> = txns.iter().map(|t| t.created_at).collect();
            sqlx::query!(
                r#"INSERT INTO attendance.leave_transactions
                     (id, user_id, grant_id, kind, delta, dayoff_id, work_pct, reason,
                      created_by_user_id, created_at)
                   SELECT u.id, u.user_id, u.grant_id, u.kind, u.delta::numeric, u.dayoff_id,
                          u.work_pct::numeric, u.reason, u.created_by_user_id, u.created_at
                   FROM UNNEST($1::uuid[], $2::uuid[], $3::uuid[],
                               $4::attendance.leave_txn_kind[], $5::float8[], $6::uuid[],
                               $7::float8[], $8::text[], $9::uuid[], $10::timestamptz[])
                     AS u(id, user_id, grant_id, kind, delta, dayoff_id, work_pct, reason,
                          created_by_user_id, created_at)"#,
                &ids,
                &user_ids,
                &grant_ids,
                kinds as Vec<SqlLeaveTxnKind>,
                &deltas,
                dayoff_ids as Vec<Option<Uuid>>,
                work_pcts as Vec<Option<f64>>,
                &reasons,
                created_bys as Vec<Option<Uuid>>,
                &created,
            )
            .execute(&mut *tx)
            .await
            .map_err(mappers::map_pg_error)?;
        }

        tx.commit().await.map_err(mappers::map_pg_error)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(asof = ?asof, within_days))]
    async fn list_expiring(
        &self,
        asof: Date,
        within_days: i64,
    ) -> Result<Vec<LeaveGrant>, RepositoryError> {
        let horizon = asof + Duration::days(within_days);
        let rows = sqlx::query_as!(
            GrantRow,
            r#"SELECT
                 id,
                 user_id,
                 grant_year,
                 days_granted::float8   AS "days_granted!",
                 days_remaining::float8 AS "days_remaining!",
                 expires_on,
                 created_by_user_id,
                 created_at,
                 updated_at
               FROM attendance.leave_grants
               WHERE days_remaining > 0 AND expires_on <= $1
               ORDER BY expires_on"#,
            horizon,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(user = ?user))]
    async fn list_transactions(
        &self,
        user: UserId,
        from: Date,
        to: Date,
    ) -> Result<Vec<LeaveTransaction>, RepositoryError> {
        let rows = sqlx::query_as!(
            TxnRow,
            r#"SELECT
                 id,
                 user_id,
                 grant_id,
                 kind AS "kind: SqlLeaveTxnKind",
                 delta::float8    AS "delta!",
                 dayoff_id,
                 work_pct::float8 AS "work_pct?",
                 reason,
                 created_by_user_id,
                 created_at
               FROM attendance.leave_transactions
               WHERE user_id = $1
                 AND created_at >= $2::date
                 AND created_at < ($3::date + 1)
               ORDER BY created_at DESC"#,
            user.0,
            from,
            to,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(dayoff = ?dayoff_id))]
    async fn transactions_for_dayoff(
        &self,
        dayoff_id: DayOffId,
    ) -> Result<Vec<LeaveTransaction>, RepositoryError> {
        let rows = sqlx::query_as!(
            TxnRow,
            r#"SELECT
                 id,
                 user_id,
                 grant_id,
                 kind AS "kind: SqlLeaveTxnKind",
                 delta::float8    AS "delta!",
                 dayoff_id,
                 work_pct::float8 AS "work_pct?",
                 reason,
                 created_by_user_id,
                 created_at
               FROM attendance.leave_transactions
               WHERE dayoff_id = $1
               ORDER BY created_at"#,
            dayoff_id.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
