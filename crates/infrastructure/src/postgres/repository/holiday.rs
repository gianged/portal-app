use async_trait::async_trait;
use domain::{error::RepositoryError, model::Holiday, repository::HolidayRepository};
use sqlx::PgPool;
use time::Date;

use crate::postgres::mappers;

pub struct PgHolidayRepo {
    pool: PgPool,
}

impl PgHolidayRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct HolidayRow {
    holiday_date: Date,
    name: String,
}

#[async_trait]
impl HolidayRepository for PgHolidayRepo {
    #[tracing::instrument(skip_all, fields(from = ?from, to = ?to))]
    async fn list(&self, from: Date, to: Date) -> Result<Vec<Holiday>, RepositoryError> {
        let rows = sqlx::query_as!(
            HolidayRow,
            r#"SELECT holiday_date, name
               FROM attendance.holidays
               WHERE holiday_date BETWEEN $1 AND $2
               ORDER BY holiday_date"#,
            from,
            to,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows
            .into_iter()
            .map(|r| Holiday {
                date: r.holiday_date,
                name: r.name,
            })
            .collect())
    }

    #[tracing::instrument(skip_all, fields(date = ?date))]
    async fn upsert(&self, date: Date, name: &str) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"INSERT INTO attendance.holidays (holiday_date, name)
               VALUES ($1, $2)
               ON CONFLICT (holiday_date) DO UPDATE SET name = EXCLUDED.name"#,
            date,
            name,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(date = ?date))]
    async fn delete(&self, date: Date) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"DELETE FROM attendance.holidays WHERE holiday_date = $1"#,
            date,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }
}
