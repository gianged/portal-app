use async_trait::async_trait;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{AuditLogId, UserId},
    model::AuditLog,
    repository::AuditRepository,
};

use crate::postgres::{enums::SqlAuditAction, mappers};

pub struct PgAuditRepo {
    pool: PgPool,
}

impl PgAuditRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct AuditRow {
    id: Uuid,
    actor_user_id: Option<Uuid>,
    action: SqlAuditAction,
    entity_schema: String,
    entity_table: String,
    entity_id: Uuid,
    payload_before: Option<String>,
    payload_after: Option<String>,
    occurred_at: OffsetDateTime,
}

impl From<AuditRow> for AuditLog {
    fn from(r: AuditRow) -> Self {
        Self {
            id: AuditLogId(r.id),
            actor_user_id: r.actor_user_id.map(UserId),
            action: r.action.into(),
            entity_schema: r.entity_schema,
            entity_table: r.entity_table,
            entity_id: r.entity_id,
            payload_before: r.payload_before,
            payload_after: r.payload_after,
            occurred_at: r.occurred_at,
        }
    }
}

#[async_trait]
impl AuditRepository for PgAuditRepo {
    #[tracing::instrument(skip_all)]
    async fn append(&self, e: &AuditLog) -> Result<(), RepositoryError> {
        // Immutable append (invariant 5): plain INSERT, no UPSERT.
        let action = SqlAuditAction::from(e.action);
        // Bind payload_* as TEXT and cast ::text::jsonb in SQL; a direct ::jsonb
        // would make sqlx infer serde_json::Value and force parsing opaque JSON.
        sqlx::query!(
            r#"INSERT INTO audit.audit_log
                 (id, actor_user_id, action, entity_schema, entity_table, entity_id,
                  payload_before, payload_after, occurred_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7::text::jsonb, $8::text::jsonb, $9)"#,
            e.id.0,
            e.actor_user_id.map(|u| u.0),
            action as SqlAuditAction,
            e.entity_schema,
            e.entity_table,
            e.entity_id,
            e.payload_before.as_deref(),
            e.payload_after.as_deref(),
            e.occurred_at,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(limit = ?limit))]
    async fn list_for_entity(
        &self,
        entity_schema: &str,
        entity_table: &str,
        entity_id: Uuid,
        limit: u32,
    ) -> Result<Vec<AuditLog>, RepositoryError> {
        // Matches idx_audit_log_entity; payload_*::text yields Postgres's canonical JSONB form.
        let rows = sqlx::query_as!(
            AuditRow,
            r#"SELECT
                 id,
                 actor_user_id,
                 action AS "action: SqlAuditAction",
                 entity_schema,
                 entity_table,
                 entity_id,
                 payload_before::text AS "payload_before?: String",
                 payload_after::text  AS "payload_after?: String",
                 occurred_at
               FROM audit.audit_log
               WHERE entity_schema = $1 AND entity_table = $2 AND entity_id = $3
               ORDER BY occurred_at DESC
               LIMIT $4"#,
            entity_schema,
            entity_table,
            entity_id,
            i64::from(limit),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(limit = ?limit))]
    async fn list_recent(
        &self,
        limit: u32,
        before: Option<OffsetDateTime>,
    ) -> Result<Vec<AuditLog>, RepositoryError> {
        // Global admin feed (idx_audit_log_occurred); NULL `before` is the first page, else page back by time.
        let rows = sqlx::query_as!(
            AuditRow,
            r#"SELECT
                 id,
                 actor_user_id,
                 action AS "action: SqlAuditAction",
                 entity_schema,
                 entity_table,
                 entity_id,
                 payload_before::text AS "payload_before?: String",
                 payload_after::text  AS "payload_after?: String",
                 occurred_at
               FROM audit.audit_log
               WHERE $1::timestamptz IS NULL OR occurred_at < $1
               ORDER BY occurred_at DESC
               LIMIT $2"#,
            before,
            i64::from(limit),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
