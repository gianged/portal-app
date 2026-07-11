use async_trait::async_trait;
use sqlx::PgPool;
use time::{OffsetDateTime, Time};
use uuid::Uuid;

use domain::{
    error::RepositoryError, ids::UserId, model::AttendancePolicy, repository::PolicyRepository,
};

use crate::postgres::{enums::SqlBalanceExpiryPolicy, mappers};

pub struct PgPolicyRepo {
    pool: PgPool,
}

impl PgPolicyRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// NUMERIC columns are read/written via ::float8 (no decimal feature).
struct PolicyRow {
    workday_start: Time,
    work_hours_per_day: f64,
    flex_core_start: Time,
    flex_core_end: Time,
    flex_daily_min: f64,
    flex_daily_max: f64,
    flex_earliest_start: Time,
    flex_latest_end: Time,
    flex_max_segments: i16,
    flex_max_per_month: i16,
    overtime_max_hours_per_month: f64,
    balance_carry_years: i16,
    balance_expiry_policy: SqlBalanceExpiryPolicy,
    balance_expiry_warn_days: i16,
    updated_by_user_id: Option<Uuid>,
    updated_at: OffsetDateTime,
}

impl From<PolicyRow> for AttendancePolicy {
    fn from(r: PolicyRow) -> Self {
        Self {
            workday_start: r.workday_start,
            work_hours_per_day: r.work_hours_per_day,
            flex_core_start: r.flex_core_start,
            flex_core_end: r.flex_core_end,
            flex_daily_min: r.flex_daily_min,
            flex_daily_max: r.flex_daily_max,
            flex_earliest_start: r.flex_earliest_start,
            flex_latest_end: r.flex_latest_end,
            flex_max_segments: u16::try_from(r.flex_max_segments).unwrap_or(0),
            flex_max_per_month: u16::try_from(r.flex_max_per_month).unwrap_or(0),
            overtime_max_hours_per_month: r.overtime_max_hours_per_month,
            balance_carry_years: u16::try_from(r.balance_carry_years).unwrap_or(1),
            balance_expiry_policy: r.balance_expiry_policy.into(),
            balance_expiry_warn_days: u16::try_from(r.balance_expiry_warn_days).unwrap_or(0),
            updated_by_user_id: r.updated_by_user_id.map(UserId),
            updated_at: r.updated_at,
        }
    }
}

#[async_trait]
impl PolicyRepository for PgPolicyRepo {
    #[tracing::instrument(skip_all)]
    async fn load(&self) -> Result<AttendancePolicy, RepositoryError> {
        sqlx::query_as!(
            PolicyRow,
            r#"SELECT
                 workday_start,
                 work_hours_per_day::float8           AS "work_hours_per_day!",
                 flex_core_start,
                 flex_core_end,
                 flex_daily_min::float8               AS "flex_daily_min!",
                 flex_daily_max::float8               AS "flex_daily_max!",
                 flex_earliest_start,
                 flex_latest_end,
                 flex_max_segments,
                 flex_max_per_month,
                 overtime_max_hours_per_month::float8 AS "overtime_max_hours_per_month!",
                 balance_carry_years,
                 balance_expiry_policy                AS "balance_expiry_policy: SqlBalanceExpiryPolicy",
                 balance_expiry_warn_days,
                 updated_by_user_id,
                 updated_at
               FROM attendance.policy
               WHERE id = true"#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
        .map(Into::into)
    }

    #[tracing::instrument(skip_all)]
    async fn save(
        &self,
        policy: &AttendancePolicy,
        updated_by: UserId,
    ) -> Result<(), RepositoryError> {
        let expiry = SqlBalanceExpiryPolicy::from(policy.balance_expiry_policy);
        let flex_max_segments = i16::try_from(policy.flex_max_segments)
            .map_err(|_| RepositoryError::Backend("flex_max_segments out of range".into()))?;
        let flex_max_per_month = i16::try_from(policy.flex_max_per_month)
            .map_err(|_| RepositoryError::Backend("flex_max_per_month out of range".into()))?;
        let balance_carry_years = i16::try_from(policy.balance_carry_years)
            .map_err(|_| RepositoryError::Backend("balance_carry_years out of range".into()))?;
        let balance_expiry_warn_days =
            i16::try_from(policy.balance_expiry_warn_days).map_err(|_| {
                RepositoryError::Backend("balance_expiry_warn_days out of range".into())
            })?;
        let result = sqlx::query!(
            r#"UPDATE attendance.policy SET
                 workday_start                = $1,
                 work_hours_per_day           = $2::float8::numeric,
                 flex_core_start              = $3,
                 flex_core_end                = $4,
                 flex_daily_min               = $5::float8::numeric,
                 flex_daily_max               = $6::float8::numeric,
                 flex_earliest_start          = $7,
                 flex_latest_end              = $8,
                 flex_max_segments            = $9,
                 flex_max_per_month           = $10,
                 overtime_max_hours_per_month = $11::float8::numeric,
                 balance_carry_years          = $12,
                 balance_expiry_policy        = $13,
                 balance_expiry_warn_days     = $14,
                 updated_by_user_id           = $15
               WHERE id = true"#,
            policy.workday_start,
            policy.work_hours_per_day,
            policy.flex_core_start,
            policy.flex_core_end,
            policy.flex_daily_min,
            policy.flex_daily_max,
            policy.flex_earliest_start,
            policy.flex_latest_end,
            flex_max_segments,
            flex_max_per_month,
            policy.overtime_max_hours_per_month,
            balance_carry_years,
            expiry as SqlBalanceExpiryPolicy,
            balance_expiry_warn_days,
            updated_by.0,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }
}
