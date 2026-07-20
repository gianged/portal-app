use async_trait::async_trait;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{ProjectId, RequestAttachmentId, RequestId, UserId},
    model::{Request, RequestAttachment, RequestStatus},
    repository::RequestRepository,
};

use crate::postgres::{
    enums::{SqlRequestPriority, SqlRequestStatus},
    mappers,
};

pub struct PgRequestRepo {
    pool: PgPool,
}

impl PgRequestRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct RequestRow {
    id: Uuid,
    project_id: Uuid,
    creator_user_id: Uuid,
    assignee_user_id: Option<Uuid>,
    title: String,
    description: String,
    status: SqlRequestStatus,
    priority: SqlRequestPriority,
    progress: i16,
    due_at: Option<OffsetDateTime>,
    completed_at: Option<OffsetDateTime>,
    version: i64,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl From<RequestRow> for Request {
    fn from(r: RequestRow) -> Self {
        Self {
            id: RequestId(r.id),
            project_id: ProjectId(r.project_id),
            creator_user_id: UserId(r.creator_user_id),
            assignee_user_id: r.assignee_user_id.map(UserId),
            title: r.title,
            description: r.description,
            status: r.status.into(),
            priority: r.priority.into(),
            progress: u8::try_from(r.progress).unwrap_or(0),
            due_at: r.due_at,
            completed_at: r.completed_at,
            version: r.version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

struct AttachmentRow {
    id: Uuid,
    request_id: Uuid,
    uploaded_by_user_id: Uuid,
    filename: String,
    content_type: String,
    size_bytes: i64,
    storage_key: String,
    created_at: OffsetDateTime,
}

impl TryFrom<AttachmentRow> for RequestAttachment {
    type Error = RepositoryError;

    fn try_from(r: AttachmentRow) -> Result<Self, Self::Error> {
        let size_bytes = u64::try_from(r.size_bytes)
            .map_err(|_| RepositoryError::Backend("negative size_bytes in row".into()))?;
        Ok(Self {
            id: RequestAttachmentId(r.id),
            request_id: RequestId(r.request_id),
            uploaded_by_user_id: UserId(r.uploaded_by_user_id),
            filename: r.filename,
            content_type: r.content_type,
            size_bytes,
            storage_key: r.storage_key,
            created_at: r.created_at,
        })
    }
}

#[async_trait]
impl RequestRepository for PgRequestRepo {
    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn find_by_id(&self, id: RequestId) -> Result<Option<Request>, RepositoryError> {
        sqlx::query_as!(
            RequestRow,
            r#"SELECT
                 id,
                 project_id,
                 creator_user_id,
                 assignee_user_id,
                 title,
                 description,
                 status   AS "status: SqlRequestStatus",
                 priority AS "priority: SqlRequestPriority",
                 progress,
                 due_at,
                 completed_at,
                 version,
                 created_at,
                 updated_at
               FROM project.requests
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    #[tracing::instrument(skip_all, fields(project = ?project, after = ?after, limit))]
    async fn list_page(
        &self,
        project: Option<ProjectId>,
        after: Option<RequestId>,
        limit: u32,
    ) -> Result<Vec<Request>, RepositoryError> {
        // Keyset over the uuid-v7 pk: stable order, no OFFSET rescans.
        let rows = sqlx::query_as!(
            RequestRow,
            r#"SELECT
                 id,
                 project_id,
                 creator_user_id,
                 assignee_user_id,
                 title,
                 description,
                 status   AS "status: SqlRequestStatus",
                 priority AS "priority: SqlRequestPriority",
                 progress,
                 due_at,
                 completed_at,
                 version,
                 created_at,
                 updated_at
               FROM project.requests
               WHERE ($1::uuid IS NULL OR project_id = $1)
                 AND ($2::uuid IS NULL OR id > $2)
               ORDER BY id
               LIMIT $3"#,
            project.map(|p| p.0),
            after.map(|a| a.0),
            i64::from(limit),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(project_id = ?project_id))]
    async fn list_for_project(
        &self,
        project_id: ProjectId,
        status: Option<RequestStatus>,
        q: Option<&str>,
    ) -> Result<Vec<Request>, RepositoryError> {
        // Matches idx_requests_project_id_status when status filter is provided.
        let status_filter: Option<SqlRequestStatus> = status.map(Into::into);
        let pattern: Option<String> = q.map(mappers::like_pattern);
        let rows = sqlx::query_as!(
            RequestRow,
            r#"SELECT
                 id,
                 project_id,
                 creator_user_id,
                 assignee_user_id,
                 title,
                 description,
                 status   AS "status: SqlRequestStatus",
                 priority AS "priority: SqlRequestPriority",
                 progress,
                 due_at,
                 completed_at,
                 version,
                 created_at,
                 updated_at
               FROM project.requests
               WHERE project_id = $1
                 AND ($2::project.request_status IS NULL OR status = $2)
                 AND ($3::text IS NULL OR title ILIKE $3)
               ORDER BY created_at DESC"#,
            project_id.0,
            status_filter as Option<SqlRequestStatus>,
            pattern,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(assignee = ?assignee))]
    async fn list_for_assignee(
        &self,
        assignee: UserId,
        status: Option<RequestStatus>,
        q: Option<&str>,
    ) -> Result<Vec<Request>, RepositoryError> {
        // Matches idx_requests_assignee_user_id_status (partial: assignee NOT NULL).
        let status_filter: Option<SqlRequestStatus> = status.map(Into::into);
        let pattern: Option<String> = q.map(mappers::like_pattern);
        let rows = sqlx::query_as!(
            RequestRow,
            r#"SELECT
                 id,
                 project_id,
                 creator_user_id,
                 assignee_user_id,
                 title,
                 description,
                 status   AS "status: SqlRequestStatus",
                 priority AS "priority: SqlRequestPriority",
                 progress,
                 due_at,
                 completed_at,
                 version,
                 created_at,
                 updated_at
               FROM project.requests
               WHERE assignee_user_id = $1
                 AND ($2::project.request_status IS NULL OR status = $2)
                 AND ($3::text IS NULL OR title ILIKE $3)
               ORDER BY created_at DESC"#,
            assignee.0,
            status_filter as Option<SqlRequestStatus>,
            pattern,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn save(&self, request: &Request) -> Result<(), RepositoryError> {
        let status = SqlRequestStatus::from(request.status);
        let priority = SqlRequestPriority::from(request.priority);
        let result = sqlx::query!(
            r#"INSERT INTO project.requests AS t
                 (id, project_id, creator_user_id, assignee_user_id, title, description,
                  status, priority, progress, due_at, completed_at, version, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
               ON CONFLICT (id) DO UPDATE SET
                 project_id        = EXCLUDED.project_id,
                 creator_user_id   = EXCLUDED.creator_user_id,
                 assignee_user_id  = EXCLUDED.assignee_user_id,
                 title             = EXCLUDED.title,
                 description       = EXCLUDED.description,
                 status            = EXCLUDED.status,
                 priority          = EXCLUDED.priority,
                 progress          = EXCLUDED.progress,
                 due_at            = EXCLUDED.due_at,
                 completed_at      = EXCLUDED.completed_at,
                 version           = EXCLUDED.version + 1
               WHERE t.version = EXCLUDED.version"#,
            request.id.0,
            request.project_id.0,
            request.creator_user_id.0,
            request.assignee_user_id.map(|u| u.0),
            request.title,
            request.description,
            status as SqlRequestStatus,
            priority as SqlRequestPriority,
            i16::from(request.progress),
            request.due_at,
            request.completed_at,
            request.version,
            request.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::Stale);
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(request_id = ?request_id))]
    async fn list_attachments(
        &self,
        request_id: RequestId,
    ) -> Result<Vec<RequestAttachment>, RepositoryError> {
        let rows = sqlx::query_as!(
            AttachmentRow,
            r#"SELECT
                 id,
                 request_id,
                 uploaded_by_user_id,
                 filename,
                 content_type,
                 size_bytes,
                 storage_key,
                 created_at
               FROM project.request_attachments
               WHERE request_id = $1
               ORDER BY created_at"#,
            request_id.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        rows.into_iter().map(RequestAttachment::try_from).collect()
    }

    #[tracing::instrument(skip_all)]
    async fn save_attachment(&self, a: &RequestAttachment) -> Result<(), RepositoryError> {
        // Write-once metadata (no updated_at); CHECK rejects non-positive sizes.
        let size_bytes = i64::try_from(a.size_bytes)
            .map_err(|_| RepositoryError::Backend("size_bytes exceeds i64::MAX".into()))?;
        sqlx::query!(
            r#"INSERT INTO project.request_attachments
                 (id, request_id, uploaded_by_user_id, filename, content_type,
                  size_bytes, storage_key, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (id) DO UPDATE SET
                 request_id          = EXCLUDED.request_id,
                 uploaded_by_user_id = EXCLUDED.uploaded_by_user_id,
                 filename            = EXCLUDED.filename,
                 content_type        = EXCLUDED.content_type,
                 size_bytes          = EXCLUDED.size_bytes,
                 storage_key         = EXCLUDED.storage_key"#,
            a.id.0,
            a.request_id.0,
            a.uploaded_by_user_id.0,
            a.filename,
            a.content_type,
            size_bytes,
            a.storage_key,
            a.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn list_all_attachment_keys(&self) -> Result<Vec<String>, RepositoryError> {
        let rows = sqlx::query!(r#"SELECT storage_key FROM project.request_attachments"#)
            .fetch_all(&self.pool)
            .await
            .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(|r| r.storage_key).collect())
    }
}
