use async_trait::async_trait;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{GroupId, MembershipId, UserId},
    model::{Group, GroupKind, Membership},
    repository::GroupRepository,
};

use crate::postgres::{
    enums::{SqlGroupKind, SqlGroupRole},
    mappers::map_pg_error,
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
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[async_trait]
impl GroupRepository for PgGroupRepo {
    async fn find_group(&self, id: GroupId) -> Result<Option<Group>, RepositoryError> {
        sqlx::query_as!(
            GroupRow,
            r#"SELECT
                 id,
                 name,
                 description,
                 kind AS "kind: SqlGroupKind",
                 created_at,
                 updated_at
               FROM org.groups
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    async fn find_it_group(&self) -> Result<Option<Group>, RepositoryError> {
        // The uq_groups_one_it partial unique index guarantees at most one row.
        let it = SqlGroupKind::from(GroupKind::It);
        sqlx::query_as!(
            GroupRow,
            r#"SELECT
                 id,
                 name,
                 description,
                 kind AS "kind: SqlGroupKind",
                 created_at,
                 updated_at
               FROM org.groups
               WHERE kind = $1
               LIMIT 1"#,
            it as SqlGroupKind,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    async fn save_group(&self, group: &Group) -> Result<(), RepositoryError> {
        let kind = SqlGroupKind::from(group.kind);
        sqlx::query!(
            r#"INSERT INTO org.groups
                 (id, name, description, kind, created_at)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (id) DO UPDATE SET
                 name        = EXCLUDED.name,
                 description = EXCLUDED.description,
                 kind        = EXCLUDED.kind"#,
            group.id.0,
            group.name,
            group.description,
            kind as SqlGroupKind,
            group.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(())
    }

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
                 created_at,
                 updated_at
               FROM org.memberships
               WHERE group_id = $1 AND user_id = $2"#,
            group_id.0,
            user_id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

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
                 created_at,
                 updated_at
               FROM org.memberships
               WHERE group_id = $1
               ORDER BY joined_at"#,
            group_id.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

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
                 created_at,
                 updated_at
               FROM org.memberships
               WHERE user_id = $1 AND deactivated_at IS NULL
               ORDER BY joined_at"#,
            user_id.0,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn save_membership(&self, m: &Membership) -> Result<(), RepositoryError> {
        let role = SqlGroupRole::from(m.role);
        sqlx::query!(
            r#"INSERT INTO org.memberships
                 (id, group_id, user_id, role, joined_at, deactivated_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               ON CONFLICT (id) DO UPDATE SET
                 group_id       = EXCLUDED.group_id,
                 user_id        = EXCLUDED.user_id,
                 role           = EXCLUDED.role,
                 joined_at      = EXCLUDED.joined_at,
                 deactivated_at = EXCLUDED.deactivated_at"#,
            m.id.0,
            m.group_id.0,
            m.user_id.0,
            role as SqlGroupRole,
            m.joined_at,
            m.deactivated_at,
            m.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(())
    }
}
