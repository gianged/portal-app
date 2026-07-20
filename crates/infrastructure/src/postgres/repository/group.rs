use async_trait::async_trait;
use sqlx::{PgExecutor, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{GroupId, MembershipId, UserId},
    model::{Group, Membership},
    repository::GroupRepository,
};

use crate::postgres::{
    enums::{SqlGroupKind, SqlGroupRole},
    mappers,
};

pub struct PgGroupRepo {
    pool: PgPool,
}

impl PgGroupRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct GroupRow {
    id: Uuid,
    name: String,
    description: String,
    kind: SqlGroupKind,
    version: i64,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl From<GroupRow> for Group {
    fn from(r: GroupRow) -> Self {
        Self {
            id: GroupId(r.id),
            name: r.name,
            description: r.description,
            kind: r.kind.into(),
            version: r.version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

struct MembershipRow {
    id: Uuid,
    group_id: Uuid,
    user_id: Uuid,
    role: SqlGroupRole,
    joined_at: OffsetDateTime,
    deactivated_at: Option<OffsetDateTime>,
    version: i64,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl From<MembershipRow> for Membership {
    fn from(r: MembershipRow) -> Self {
        Self {
            id: MembershipId(r.id),
            group_id: GroupId(r.group_id),
            user_id: UserId(r.user_id),
            role: r.role.into(),
            joined_at: r.joined_at,
            deactivated_at: r.deactivated_at,
            version: r.version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

// Single upsert statement shared by single and batched saves so the sqlx
// statement cache serves both. Returns rows affected; 0 = version guard missed.
async fn upsert_membership(
    executor: impl PgExecutor<'_>,
    m: &Membership,
) -> Result<u64, sqlx::Error> {
    let role = SqlGroupRole::from(m.role);
    // Literal kept byte-identical to the committed .sqlx cache entry.
    let result = sqlx::query!(
        r#"INSERT INTO org.memberships AS t
                 (id, group_id, user_id, role, joined_at, deactivated_at, version, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
               ON CONFLICT (id) DO UPDATE SET
                 group_id       = EXCLUDED.group_id,
                 user_id        = EXCLUDED.user_id,
                 role           = EXCLUDED.role,
                 joined_at      = EXCLUDED.joined_at,
                 deactivated_at = EXCLUDED.deactivated_at,
                 version        = EXCLUDED.version + 1
               WHERE t.version = EXCLUDED.version"#,
        m.id.0,
        m.group_id.0,
        m.user_id.0,
        role as SqlGroupRole,
        m.joined_at,
        m.deactivated_at,
        m.version,
        m.created_at,
    )
    .execute(executor)
    .await?;
    Ok(result.rows_affected())
}

#[async_trait]
impl GroupRepository for PgGroupRepo {
    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn find_group(&self, id: GroupId) -> Result<Option<Group>, RepositoryError> {
        sqlx::query_as!(
            GroupRow,
            r#"SELECT
                 id,
                 name,
                 description,
                 kind AS "kind: SqlGroupKind",
                 version,
                 created_at,
                 updated_at
               FROM org.groups
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    #[tracing::instrument(skip_all, fields(count = ids.len()))]
    async fn find_by_ids(&self, ids: &[GroupId]) -> Result<Vec<Group>, RepositoryError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let raw: Vec<Uuid> = ids.iter().map(|id| id.0).collect();
        let rows = sqlx::query_as!(
            GroupRow,
            r#"SELECT
                 id,
                 name,
                 description,
                 kind AS "kind: SqlGroupKind",
                 version,
                 created_at,
                 updated_at
               FROM org.groups
               WHERE id = ANY($1)"#,
            &raw,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn list_all(&self) -> Result<Vec<Group>, RepositoryError> {
        let rows = sqlx::query_as!(
            GroupRow,
            r#"SELECT
                 id,
                 name,
                 description,
                 kind AS "kind: SqlGroupKind",
                 version,
                 created_at,
                 updated_at
               FROM org.groups
               ORDER BY name"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn find_it_group(&self) -> Result<Option<Group>, RepositoryError> {
        // The uq_groups_one_it partial unique index guarantees at most one row.
        sqlx::query_as!(
            GroupRow,
            r#"SELECT
                 id,
                 name,
                 description,
                 kind AS "kind: SqlGroupKind",
                 version,
                 created_at,
                 updated_at
               FROM org.groups
               WHERE kind = 'it'
               LIMIT 1"#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    #[tracing::instrument(skip_all)]
    async fn save_group(&self, group: &Group) -> Result<(), RepositoryError> {
        let kind = SqlGroupKind::from(group.kind);
        let result = sqlx::query!(
            r#"INSERT INTO org.groups AS t
                 (id, name, description, kind, version, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (id) DO UPDATE SET
                 name        = EXCLUDED.name,
                 description = EXCLUDED.description,
                 kind        = EXCLUDED.kind,
                 version     = EXCLUDED.version + 1
               WHERE t.version = EXCLUDED.version"#,
            group.id.0,
            group.name,
            group.description,
            kind as SqlGroupKind,
            group.version,
            group.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::Stale);
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(group_id = ?group_id, user_id = ?user_id))]
    async fn find_membership(
        &self,
        group_id: GroupId,
        user_id: UserId,
    ) -> Result<Option<Membership>, RepositoryError> {
        sqlx::query_as!(
            MembershipRow,
            r#"SELECT
                 id,
                 group_id,
                 user_id,
                 role AS "role: SqlGroupRole",
                 joined_at,
                 deactivated_at,
                 version,
                 created_at,
                 updated_at
               FROM org.memberships
               WHERE group_id = $1 AND user_id = $2"#,
            group_id.0,
            user_id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    #[tracing::instrument(skip_all, fields(group_id = ?group_id))]
    async fn list_memberships_for_group(
        &self,
        group_id: GroupId,
    ) -> Result<Vec<Membership>, RepositoryError> {
        let rows = sqlx::query_as!(
            MembershipRow,
            r#"SELECT
                 id,
                 group_id,
                 user_id,
                 role AS "role: SqlGroupRole",
                 joined_at,
                 deactivated_at,
                 version,
                 created_at,
                 updated_at
               FROM org.memberships
               WHERE group_id = $1
               ORDER BY joined_at"#,
            group_id.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(user_id = ?user_id))]
    async fn list_active_memberships_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Vec<Membership>, RepositoryError> {
        // The `deactivated_at IS NULL` filter matches idx_memberships_user_id_active.
        let rows = sqlx::query_as!(
            MembershipRow,
            r#"SELECT
                 id,
                 group_id,
                 user_id,
                 role AS "role: SqlGroupRole",
                 joined_at,
                 deactivated_at,
                 version,
                 created_at,
                 updated_at
               FROM org.memberships
               WHERE user_id = $1 AND deactivated_at IS NULL
               ORDER BY joined_at"#,
            user_id.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn list_active_memberships_for_users(
        &self,
        user_ids: &[UserId],
    ) -> Result<Vec<Membership>, RepositoryError> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = user_ids.iter().map(|u| u.0).collect();
        let rows = sqlx::query_as!(
            MembershipRow,
            r#"SELECT
                 id,
                 group_id,
                 user_id,
                 role AS "role: SqlGroupRole",
                 joined_at,
                 deactivated_at,
                 version,
                 created_at,
                 updated_at
               FROM org.memberships
               WHERE user_id = ANY($1) AND deactivated_at IS NULL
               ORDER BY joined_at"#,
            &ids,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn save_membership(&self, m: &Membership) -> Result<(), RepositoryError> {
        let rows = upsert_membership(&self.pool, m)
            .await
            .map_err(mappers::map_pg_error)?;
        if rows == 0 {
            return Err(RepositoryError::Stale);
        }
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(count = memberships.len()))]
    async fn save_memberships(&self, memberships: &[Membership]) -> Result<(), RepositoryError> {
        let mut tx = self.pool.begin().await.map_err(mappers::map_pg_error)?;
        for m in memberships {
            let rows = upsert_membership(&mut *tx, m)
                .await
                .map_err(mappers::map_pg_error)?;
            // One stale row voids the whole batch so a leader swap stays atomic.
            if rows == 0 {
                tx.rollback().await.map_err(mappers::map_pg_error)?;
                return Err(RepositoryError::Stale);
            }
        }
        tx.commit().await.map_err(mappers::map_pg_error)?;
        Ok(())
    }
}
