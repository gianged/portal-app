use async_trait::async_trait;
use sqlx::{PgPool, types::Json};
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{NotificationId, UserId},
    model::{Notification, NotificationPayload},
    repository::NotificationRepository,
};

use crate::postgres::{enums::SqlNotificationKind, mappers::map_pg_error};

pub struct PgNotificationRepo {
    pool: PgPool,
}

impl PgNotificationRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct NotificationRow {
    id: Uuid,
    recipient_user_id: Uuid,
    payload: Json<NotificationPayload>,
    read_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
}

impl From<NotificationRow> for Notification {
    fn from(r: NotificationRow) -> Self {
        Self {
            id: NotificationId(r.id),
            recipient_user_id: UserId(r.recipient_user_id),
            payload: r.payload.0,
            read_at: r.read_at,
            created_at: r.created_at,
        }
    }
}

#[async_trait]
impl NotificationRepository for PgNotificationRepo {
    async fn find_by_id(
        &self,
        id: NotificationId,
    ) -> Result<Option<Notification>, RepositoryError> {
        sqlx::query_as!(
            NotificationRow,
            r#"SELECT
                 id,
                 recipient_user_id,
                 payload AS "payload: Json<NotificationPayload>",
                 read_at,
                 created_at
               FROM notification.notifications
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    async fn list_for_user(
        &self,
        user_id: UserId,
        unread_only: bool,
        limit: u32,
    ) -> Result<Vec<Notification>, RepositoryError> {
        // unread=true matches idx_notifications_recipient_user_id_unread (partial);
        // unread=false matches idx_notifications_recipient_user_id_created.
        let rows = sqlx::query_as!(
            NotificationRow,
            r#"SELECT
                 id,
                 recipient_user_id,
                 payload AS "payload: Json<NotificationPayload>",
                 read_at,
                 created_at
               FROM notification.notifications
               WHERE recipient_user_id = $1
                 AND ($2 = false OR read_at IS NULL)
               ORDER BY created_at DESC
               LIMIT $3"#,
            user_id.0,
            unread_only,
            i64::from(limit),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn count_unread(&self, user_id: UserId) -> Result<u64, RepositoryError> {
        let row = sqlx::query!(
            r#"SELECT COUNT(*) AS "count!"
               FROM notification.notifications
               WHERE recipient_user_id = $1 AND read_at IS NULL"#,
            user_id.0,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_pg_error)?;
        u64::try_from(row.count).map_err(|_| RepositoryError::Backend("negative count".into()))
    }

    async fn save(&self, n: &Notification) -> Result<(), RepositoryError> {
        let kind = SqlNotificationKind::from(n.kind());
        let payload = Json(&n.payload);
        sqlx::query!(
            r#"INSERT INTO notification.notifications
                 (id, recipient_user_id, kind, payload, read_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (id) DO UPDATE SET
                 recipient_user_id = EXCLUDED.recipient_user_id,
                 kind              = EXCLUDED.kind,
                 payload           = EXCLUDED.payload,
                 read_at           = EXCLUDED.read_at"#,
            n.id.0,
            n.recipient_user_id.0,
            kind as SqlNotificationKind,
            payload as Json<&NotificationPayload>,
            n.read_at,
            n.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(())
    }

    async fn mark_read(
        &self,
        id: NotificationId,
        at: OffsetDateTime,
    ) -> Result<(), RepositoryError> {
        // Idempotent: first call sets read_at; subsequent calls are no-ops because
        // the WHERE clause filters to read_at IS NULL. The original timestamp is preserved.
        sqlx::query!(
            r#"UPDATE notification.notifications
               SET read_at = $2
               WHERE id = $1 AND read_at IS NULL"#,
            id.0,
            at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(())
    }

    async fn delete_read_before(&self, cutoff: OffsetDateTime) -> Result<u64, RepositoryError> {
        let result = sqlx::query!(
            r#"DELETE FROM notification.notifications
               WHERE read_at IS NOT NULL AND read_at < $1"#,
            cutoff,
        )
        .execute(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(result.rows_affected())
    }
}
