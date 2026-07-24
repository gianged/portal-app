use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    repository::{OutboxRecord, OutboxRepository},
};

use crate::postgres::mappers;

pub struct PgOutboxRepo {
    pool: PgPool,
}

impl PgOutboxRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OutboxRepository for PgOutboxRepo {
    #[tracing::instrument(skip_all, fields(limit))]
    async fn claim_unprocessed(&self, limit: u32) -> Result<Vec<OutboxRecord>, RepositoryError> {
        // Plain read (no lock): projection dedups on the record id, so a double
        // claim converges. Matches idx_outbox_events_unprocessed.
        let rows = sqlx::query!(
            r#"SELECT id, topic, payload
               FROM audit.outbox_events
               WHERE processed_at IS NULL
               ORDER BY created_at
               LIMIT $1"#,
            i64::from(limit),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows
            .into_iter()
            .map(|r| OutboxRecord {
                id: r.id,
                topic: r.topic,
                payload: r.payload,
            })
            .collect())
    }

    #[tracing::instrument(skip_all, fields(id = %id))]
    async fn mark_processed(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"UPDATE audit.outbox_events SET processed_at = NOW() WHERE id = $1"#,
            id,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }
}
