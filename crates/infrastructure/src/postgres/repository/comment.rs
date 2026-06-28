use async_trait::async_trait;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{CommentId, RequestId, TicketId, UserId},
    model::{Comment, CommentEntity},
    repository::CommentRepository,
};

use crate::postgres::mappers;

/// One repo over both comment tables; each method matches the entity to pick the table.
pub struct PgCommentRepo {
    pool: PgPool,
}

impl PgCommentRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct CommentRow {
    id: Uuid,
    author_user_id: Uuid,
    body: String,
    edited_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
}

impl CommentRow {
    /// Reattaches the entity since the source table already implies the parent.
    fn into_comment(self, entity: CommentEntity) -> Comment {
        Comment {
            id: CommentId(self.id),
            entity,
            author_user_id: UserId(self.author_user_id),
            body: self.body,
            edited_at: self.edited_at,
            created_at: self.created_at,
        }
    }
}

#[async_trait]
impl CommentRepository for PgCommentRepo {
    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn find_by_id(
        &self,
        entity: CommentEntity,
        id: CommentId,
    ) -> Result<Option<Comment>, RepositoryError> {
        let row = match entity {
            CommentEntity::Request { request_id } => {
                sqlx::query_as!(
                    CommentRow,
                    r#"SELECT id, author_user_id, body, edited_at, created_at
                       FROM project.request_comments
                       WHERE request_id = $1 AND id = $2"#,
                    request_id.0,
                    id.0,
                )
                .fetch_optional(&self.pool)
                .await
            }
            CommentEntity::Ticket { ticket_id } => {
                sqlx::query_as!(
                    CommentRow,
                    r#"SELECT id, author_user_id, body, edited_at, created_at
                       FROM ticket.ticket_comments
                       WHERE ticket_id = $1 AND id = $2"#,
                    ticket_id.0,
                    id.0,
                )
                .fetch_optional(&self.pool)
                .await
            }
        }
        .map_err(mappers::map_pg_error)?;
        Ok(row.map(|r| r.into_comment(entity)))
    }

    #[tracing::instrument(skip_all, fields(limit = ?limit))]
    async fn list_for_entity(
        &self,
        entity: CommentEntity,
        before: Option<CommentId>,
        limit: u32,
    ) -> Result<Vec<Comment>, RepositoryError> {
        // UUIDv7 ids are time-ordered, so `id < before ORDER BY id DESC` is newest-first cursor pagination.
        let cursor = before.map(|c| c.0);
        let rows = match entity {
            CommentEntity::Request { request_id } => {
                sqlx::query_as!(
                    CommentRow,
                    r#"SELECT id, author_user_id, body, edited_at, created_at
                       FROM project.request_comments
                       WHERE request_id = $1 AND ($2::uuid IS NULL OR id < $2)
                       ORDER BY id DESC
                       LIMIT $3"#,
                    request_id.0,
                    cursor,
                    i64::from(limit),
                )
                .fetch_all(&self.pool)
                .await
            }
            CommentEntity::Ticket { ticket_id } => {
                sqlx::query_as!(
                    CommentRow,
                    r#"SELECT id, author_user_id, body, edited_at, created_at
                       FROM ticket.ticket_comments
                       WHERE ticket_id = $1 AND ($2::uuid IS NULL OR id < $2)
                       ORDER BY id DESC
                       LIMIT $3"#,
                    ticket_id.0,
                    cursor,
                    i64::from(limit),
                )
                .fetch_all(&self.pool)
                .await
            }
        }
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(|r| r.into_comment(entity)).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn save(&self, comment: &Comment) -> Result<(), RepositoryError> {
        // Author/parent/created_at never change; only the grace-window edit does.
        match comment.entity {
            CommentEntity::Request { request_id } => self
                .save_request_comment(comment, request_id)
                .await
                .map_err(mappers::map_pg_error),
            CommentEntity::Ticket { ticket_id } => self
                .save_ticket_comment(comment, ticket_id)
                .await
                .map_err(mappers::map_pg_error),
        }
    }

    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn delete(&self, entity: CommentEntity, id: CommentId) -> Result<(), RepositoryError> {
        match entity {
            CommentEntity::Request { request_id } => {
                sqlx::query!(
                    r#"DELETE FROM project.request_comments WHERE request_id = $1 AND id = $2"#,
                    request_id.0,
                    id.0,
                )
                .execute(&self.pool)
                .await
            }
            CommentEntity::Ticket { ticket_id } => {
                sqlx::query!(
                    r#"DELETE FROM ticket.ticket_comments WHERE ticket_id = $1 AND id = $2"#,
                    ticket_id.0,
                    id.0,
                )
                .execute(&self.pool)
                .await
            }
        }
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }
}

impl PgCommentRepo {
    async fn save_request_comment(
        &self,
        comment: &Comment,
        request_id: RequestId,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"INSERT INTO project.request_comments
                 (id, request_id, author_user_id, body, edited_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (id) DO UPDATE SET
                 body      = EXCLUDED.body,
                 edited_at = EXCLUDED.edited_at"#,
            comment.id.0,
            request_id.0,
            comment.author_user_id.0,
            comment.body,
            comment.edited_at,
            comment.created_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn save_ticket_comment(
        &self,
        comment: &Comment,
        ticket_id: TicketId,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"INSERT INTO ticket.ticket_comments
                 (id, ticket_id, author_user_id, body, edited_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (id) DO UPDATE SET
                 body      = EXCLUDED.body,
                 edited_at = EXCLUDED.edited_at"#,
            comment.id.0,
            ticket_id.0,
            comment.author_user_id.0,
            comment.body,
            comment.edited_at,
            comment.created_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
