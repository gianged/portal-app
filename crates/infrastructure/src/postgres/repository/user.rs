use async_trait::async_trait;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::UserId,
    model::{User, UserStatus},
    repository::UserRepository,
};

use crate::postgres::{
    enums::{SqlSystemRole, SqlUserStatus},
    mappers::{like_pattern, map_pg_error},
};

pub struct PgUserRepo {
    pool: PgPool,
}

impl PgUserRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct UserRow {
    id: Uuid,
    email: String,
    password_hash: String,
    full_name: String,
    avatar_storage_key: Option<String>,
    phone: Option<String>,
    timezone: String,
    status: SqlUserStatus,
    system_role: Option<SqlSystemRole>,
    first_logged_in_at: Option<OffsetDateTime>,
    deactivated_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl From<UserRow> for User {
    fn from(r: UserRow) -> Self {
        Self {
            id: UserId(r.id),
            email: r.email,
            password_hash: r.password_hash,
            full_name: r.full_name,
            avatar_storage_key: r.avatar_storage_key,
            phone: r.phone,
            timezone: r.timezone,
            status: r.status.into(),
            system_role: r.system_role.map(Into::into),
            first_logged_in_at: r.first_logged_in_at,
            deactivated_at: r.deactivated_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[async_trait]
impl UserRepository for PgUserRepo {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError> {
        sqlx::query_as!(
            UserRow,
            r#"SELECT
                 id,
                 email,
                 password_hash,
                 full_name,
                 avatar_storage_key,
                 phone,
                 timezone,
                 status            AS "status: SqlUserStatus",
                 system_role       AS "system_role: SqlSystemRole",
                 first_logged_in_at,
                 deactivated_at,
                 created_at,
                 updated_at
               FROM auth.users
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, RepositoryError> {
        sqlx::query_as!(
            UserRow,
            r#"SELECT
                 id,
                 email,
                 password_hash,
                 full_name,
                 avatar_storage_key,
                 phone,
                 timezone,
                 status            AS "status: SqlUserStatus",
                 system_role       AS "system_role: SqlSystemRole",
                 first_logged_in_at,
                 deactivated_at,
                 created_at,
                 updated_at
               FROM auth.users
               WHERE email = $1"#,
            email,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    async fn list_active(
        &self,
        limit: u32,
        offset: u32,
        q: Option<&str>,
    ) -> Result<Vec<User>, RepositoryError> {
        let active = SqlUserStatus::from(UserStatus::Active);
        let pattern: Option<String> = q.map(like_pattern);
        let rows = sqlx::query_as!(
            UserRow,
            r#"SELECT
                 id,
                 email,
                 password_hash,
                 full_name,
                 avatar_storage_key,
                 phone,
                 timezone,
                 status            AS "status: SqlUserStatus",
                 system_role       AS "system_role: SqlSystemRole",
                 first_logged_in_at,
                 deactivated_at,
                 created_at,
                 updated_at
               FROM auth.users
               WHERE status = $1
                 AND ($4::text IS NULL OR full_name ILIKE $4 OR email ILIKE $4)
               ORDER BY created_at
               LIMIT $2
               OFFSET $3"#,
            active as SqlUserStatus,
            i64::from(limit),
            i64::from(offset),
            pattern,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn save(&self, user: &User) -> Result<(), RepositoryError> {
        let status = SqlUserStatus::from(user.status);
        let system_role: Option<SqlSystemRole> = user.system_role.map(Into::into);
        sqlx::query!(
            r#"INSERT INTO auth.users
                 (id, email, password_hash, full_name, avatar_storage_key, phone,
                  timezone, status, system_role, first_logged_in_at, deactivated_at,
                  created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
               ON CONFLICT (id) DO UPDATE SET
                 email              = EXCLUDED.email,
                 password_hash      = EXCLUDED.password_hash,
                 full_name          = EXCLUDED.full_name,
                 avatar_storage_key = EXCLUDED.avatar_storage_key,
                 phone              = EXCLUDED.phone,
                 timezone           = EXCLUDED.timezone,
                 status             = EXCLUDED.status,
                 system_role        = EXCLUDED.system_role,
                 first_logged_in_at = EXCLUDED.first_logged_in_at,
                 deactivated_at     = EXCLUDED.deactivated_at"#,
            user.id.0,
            user.email,
            user.password_hash,
            user.full_name,
            user.avatar_storage_key,
            user.phone,
            user.timezone,
            status as SqlUserStatus,
            system_role as Option<SqlSystemRole>,
            user.first_logged_in_at,
            user.deactivated_at,
            user.created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(())
    }

    async fn list_avatar_keys(&self) -> Result<Vec<String>, RepositoryError> {
        let rows = sqlx::query!(
            r#"SELECT avatar_storage_key AS "avatar_storage_key!"
               FROM auth.users
               WHERE avatar_storage_key IS NOT NULL"#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_pg_error)?;
        Ok(rows.into_iter().map(|r| r.avatar_storage_key).collect())
    }
}
