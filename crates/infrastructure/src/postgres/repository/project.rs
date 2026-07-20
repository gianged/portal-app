use async_trait::async_trait;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{GroupId, ProjectCollaboratorId, ProjectId, ProjectInviteId, UserId},
    model::{Project, ProjectCollaborator, ProjectInvite},
    repository::ProjectRepository,
};

use crate::postgres::{
    enums::{SqlInviteStatus, SqlProjectStatus},
    mappers,
};

pub struct PgProjectRepo {
    pool: PgPool,
}

impl PgProjectRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct ProjectRow {
    id: Uuid,
    owner_group_id: Uuid,
    created_by_user_id: Uuid,
    name: String,
    description: String,
    status: SqlProjectStatus,
    progress: i16,
    completed_at: Option<OffsetDateTime>,
    version: i64,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl From<ProjectRow> for Project {
    fn from(r: ProjectRow) -> Self {
        Self {
            id: ProjectId(r.id),
            owner_group_id: GroupId(r.owner_group_id),
            created_by_user_id: UserId(r.created_by_user_id),
            name: r.name,
            description: r.description,
            status: r.status.into(),
            // CHECK constrains the column to 0..=100, so the cast never truncates.
            progress: u8::try_from(r.progress).unwrap_or(0),
            completed_at: r.completed_at,
            version: r.version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

struct CollaboratorRow {
    id: Uuid,
    project_id: Uuid,
    group_id: Uuid,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl From<CollaboratorRow> for ProjectCollaborator {
    fn from(r: CollaboratorRow) -> Self {
        Self {
            id: ProjectCollaboratorId(r.id),
            project_id: ProjectId(r.project_id),
            group_id: GroupId(r.group_id),
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

struct InviteRow {
    id: Uuid,
    project_id: Uuid,
    invited_by_user_id: Uuid,
    invited_group_id: Uuid,
    responded_by_user_id: Option<Uuid>,
    status: SqlInviteStatus,
    responded_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl From<InviteRow> for ProjectInvite {
    fn from(r: InviteRow) -> Self {
        Self {
            id: ProjectInviteId(r.id),
            project_id: ProjectId(r.project_id),
            invited_by_user_id: UserId(r.invited_by_user_id),
            invited_group_id: GroupId(r.invited_group_id),
            responded_by_user_id: r.responded_by_user_id.map(UserId),
            status: r.status.into(),
            responded_at: r.responded_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[async_trait]
impl ProjectRepository for PgProjectRepo {
    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn find_by_id(&self, id: ProjectId) -> Result<Option<Project>, RepositoryError> {
        sqlx::query_as!(
            ProjectRow,
            r#"SELECT
                 id,
                 owner_group_id,
                 created_by_user_id,
                 name,
                 description,
                 status AS "status: SqlProjectStatus",
                 progress,
                 completed_at,
                 version,
                 created_at,
                 updated_at
               FROM project.projects
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    #[tracing::instrument(skip_all, fields(after = ?after, limit))]
    async fn list_page(
        &self,
        after: Option<ProjectId>,
        limit: u32,
    ) -> Result<Vec<Project>, RepositoryError> {
        // Keyset over the uuid-v7 pk: stable order, no OFFSET rescans.
        let rows = sqlx::query_as!(
            ProjectRow,
            r#"SELECT
                 id,
                 owner_group_id,
                 created_by_user_id,
                 name,
                 description,
                 status AS "status: SqlProjectStatus",
                 progress,
                 completed_at,
                 version,
                 created_at,
                 updated_at
               FROM project.projects
               WHERE $1::uuid IS NULL OR id > $1
               ORDER BY id
               LIMIT $2"#,
            after.map(|a| a.0),
            i64::from(limit),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(group_id = ?group_id))]
    async fn list_for_owner_group(
        &self,
        group_id: GroupId,
        q: Option<&str>,
    ) -> Result<Vec<Project>, RepositoryError> {
        // Matches idx_projects_owner_group_id_status.
        let pattern: Option<String> = q.map(mappers::like_pattern);
        let rows = sqlx::query_as!(
            ProjectRow,
            r#"SELECT
                 id,
                 owner_group_id,
                 created_by_user_id,
                 name,
                 description,
                 status AS "status: SqlProjectStatus",
                 progress,
                 completed_at,
                 version,
                 created_at,
                 updated_at
               FROM project.projects
               WHERE owner_group_id = $1
                 AND ($2::text IS NULL OR name ILIKE $2)
               ORDER BY created_at DESC"#,
            group_id.0,
            pattern,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(group_id = ?group_id))]
    async fn list_for_collaborator_group(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<Project>, RepositoryError> {
        let rows = sqlx::query_as!(
            ProjectRow,
            r#"SELECT
                 p.id,
                 p.owner_group_id,
                 p.created_by_user_id,
                 p.name,
                 p.description,
                 p.status AS "status: SqlProjectStatus",
                 p.progress,
                 p.completed_at,
                 p.version,
                 p.created_at,
                 p.updated_at
               FROM project.projects p
               JOIN project.project_collaborators c
                 ON c.project_id = p.id
               WHERE c.group_id = $1
               ORDER BY p.created_at DESC"#,
            group_id.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn save_project(&self, project: &Project) -> Result<(), RepositoryError> {
        let status = SqlProjectStatus::from(project.status);
        let result = sqlx::query!(
            r#"INSERT INTO project.projects AS t
                 (id, owner_group_id, created_by_user_id, name, description, status,
                  progress, completed_at, version, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
               ON CONFLICT (id) DO UPDATE SET
                 owner_group_id     = EXCLUDED.owner_group_id,
                 created_by_user_id = EXCLUDED.created_by_user_id,
                 name               = EXCLUDED.name,
                 description        = EXCLUDED.description,
                 status             = EXCLUDED.status,
                 progress           = EXCLUDED.progress,
                 completed_at       = EXCLUDED.completed_at,
                 version            = EXCLUDED.version + 1
               WHERE t.version = EXCLUDED.version"#,
            project.id.0,
            project.owner_group_id.0,
            project.created_by_user_id.0,
            project.name,
            project.description,
            status as SqlProjectStatus,
            i16::from(project.progress),
            project.completed_at,
            project.version,
            project.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::Stale);
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(project_id = ?project_id))]
    async fn list_collaborators(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectCollaborator>, RepositoryError> {
        let rows = sqlx::query_as!(
            CollaboratorRow,
            r#"SELECT id, project_id, group_id, created_at, updated_at
               FROM project.project_collaborators
               WHERE project_id = $1
               ORDER BY created_at"#,
            project_id.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn save_collaborator(&self, c: &ProjectCollaborator) -> Result<(), RepositoryError> {
        // fn_no_self_collab trigger blocks owner-as-collaborator, surfacing as a CheckViolation.
        sqlx::query!(
            r#"INSERT INTO project.project_collaborators
                 (id, project_id, group_id, created_at)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (id) DO UPDATE SET
                 project_id = EXCLUDED.project_id,
                 group_id   = EXCLUDED.group_id"#,
            c.id.0,
            c.project_id.0,
            c.group_id.0,
            c.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn delete_collaborator(&self, id: ProjectCollaboratorId) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"DELETE FROM project.project_collaborators WHERE id = $1"#,
            id.0,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn find_invite(
        &self,
        id: ProjectInviteId,
    ) -> Result<Option<ProjectInvite>, RepositoryError> {
        sqlx::query_as!(
            InviteRow,
            r#"SELECT
                 id,
                 project_id,
                 invited_by_user_id,
                 invited_group_id,
                 responded_by_user_id,
                 status AS "status: SqlInviteStatus",
                 responded_at,
                 created_at,
                 updated_at
               FROM project.project_invites
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    #[tracing::instrument(skip_all, fields(group_id = ?group_id))]
    async fn list_pending_invites_for_group(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<ProjectInvite>, RepositoryError> {
        // Matches idx_project_invites_invited_group_id_status.
        let rows = sqlx::query_as!(
            InviteRow,
            r#"SELECT
                 id,
                 project_id,
                 invited_by_user_id,
                 invited_group_id,
                 responded_by_user_id,
                 status AS "status: SqlInviteStatus",
                 responded_at,
                 created_at,
                 updated_at
               FROM project.project_invites
               WHERE invited_group_id = $1 AND status = 'pending'
               ORDER BY created_at DESC"#,
            group_id.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(project_id = ?project_id))]
    async fn list_pending_invites_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<ProjectInvite>, RepositoryError> {
        let rows = sqlx::query_as!(
            InviteRow,
            r#"SELECT
                 id,
                 project_id,
                 invited_by_user_id,
                 invited_group_id,
                 responded_by_user_id,
                 status AS "status: SqlInviteStatus",
                 responded_at,
                 created_at,
                 updated_at
               FROM project.project_invites
               WHERE project_id = $1 AND status = 'pending'
               ORDER BY created_at DESC"#,
            project_id.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn save_invite(&self, invite: &ProjectInvite) -> Result<(), RepositoryError> {
        let status = SqlInviteStatus::from(invite.status);
        sqlx::query!(
            r#"INSERT INTO project.project_invites
                 (id, project_id, invited_by_user_id, invited_group_id,
                  responded_by_user_id, status, responded_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (id) DO UPDATE SET
                 project_id           = EXCLUDED.project_id,
                 invited_by_user_id   = EXCLUDED.invited_by_user_id,
                 invited_group_id     = EXCLUDED.invited_group_id,
                 responded_by_user_id = EXCLUDED.responded_by_user_id,
                 status               = EXCLUDED.status,
                 responded_at         = EXCLUDED.responded_at"#,
            invite.id.0,
            invite.project_id.0,
            invite.invited_by_user_id.0,
            invite.invited_group_id.0,
            invite.responded_by_user_id.map(|u| u.0),
            status as SqlInviteStatus,
            invite.responded_at,
            invite.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }
}
