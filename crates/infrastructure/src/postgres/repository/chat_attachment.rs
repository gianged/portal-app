use async_trait::async_trait;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{ChannelId, ChatAttachmentId, UserId},
    model::ChatAttachment,
    repository::ChatAttachmentRepository,
};

use crate::postgres::mappers::map_pg_error;

pub struct PgChatAttachmentRepo {
    pool: PgPool,
}

impl PgChatAttachmentRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct AttachmentRow {
    id: Uuid,
    channel_id: Uuid,
    uploaded_by_user_id: Uuid,
    filename: String,
    content_type: String,
    size_bytes: i64,
    storage_key: String,
    created_at: OffsetDateTime,
}

impl TryFrom<AttachmentRow> for ChatAttachment {
    type Error = RepositoryError;

    fn try_from(r: AttachmentRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: ChatAttachmentId(r.id),
            channel_id: ChannelId(r.channel_id),
            uploaded_by_user_id: UserId(r.uploaded_by_user_id),
            filename: r.filename,
            content_type: r.content_type,
            size_bytes: u64::try_from(r.size_bytes)
                .map_err(|_| RepositoryError::Backend("negative size_bytes".into()))?,
            storage_key: r.storage_key,
            created_at: r.created_at,
        })
    }
}

#[async_trait]
impl ChatAttachmentRepository for PgChatAttachmentRepo {
    async fn save(&self, a: &ChatAttachment) -> Result<(), RepositoryError> {
        // Write-once metadata (like request_attachments); the CHECK constraint
        // catches non-positive sizes.
        let size_bytes = i64::try_from(a.size_bytes)
            .map_err(|_| RepositoryError::Backend("size_bytes exceeds i64::MAX".into()))?;
        sqlx::query!(
            r#"INSERT INTO chat.message_attachments
                 (id, channel_id, uploaded_by_user_id, filename, content_type,
                  size_bytes, storage_key, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (id) DO UPDATE SET
                 channel_id          = EXCLUDED.channel_id,
                 uploaded_by_user_id = EXCLUDED.uploaded_by_user_id,
                 filename            = EXCLUDED.filename,
                 content_type        = EXCLUDED.content_type,
                 size_bytes          = EXCLUDED.size_bytes,
                 storage_key         = EXCLUDED.storage_key"#,
            a.id.0,
            a.channel_id.0,
            a.uploaded_by_user_id.0,
            a.filename,
            a.content_type,
            size_bytes,
            a.storage_key,
            a.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(())
    }

    async fn find_by_keys(&self, keys: &[String]) -> Result<Vec<ChatAttachment>, RepositoryError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_as!(
            AttachmentRow,
            r#"SELECT id, channel_id, uploaded_by_user_id, filename, content_type,
                      size_bytes, storage_key, created_at
               FROM chat.message_attachments
               WHERE storage_key = ANY($1)"#,
            keys,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_pg_error)?;
        rows.into_iter().map(ChatAttachment::try_from).collect()
    }

    async fn list_all_keys(&self) -> Result<Vec<String>, RepositoryError> {
        let rows = sqlx::query!(r#"SELECT storage_key FROM chat.message_attachments"#)
            .fetch_all(&self.pool)
            .await
            .map_err(map_pg_error)?;
        Ok(rows.into_iter().map(|r| r.storage_key).collect())
    }
}
