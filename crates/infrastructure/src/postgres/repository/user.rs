use async_trait::async_trait;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::UserId,
    model::User,
    repository::{OutboxRecord, UserRepository},
};

use crate::postgres::{
    enums::{SqlSystemRole, SqlUserStatus},
    mappers, outbox,
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
    email_notifications: bool,
    first_logged_in_at: Option<OffsetDateTime>,
    deactivated_at: Option<OffsetDateTime>,
    version: i64,
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
            email_notifications: r.email_notifications,
            first_logged_in_at: r.first_logged_in_at,
            deactivated_at: r.deactivated_at,
            version: r.version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[async_trait]
impl UserRepository for PgUserRepo {
    #[tracing::instrument(skip_all, fields(id = ?id))]
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
                 email_notifications,
                 first_logged_in_at,
                 deactivated_at,
                 version,
                 created_at,
                 updated_at
               FROM auth.users
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    #[tracing::instrument(skip_all, fields(count = ids.len()))]
    async fn find_by_ids(&self, ids: &[UserId]) -> Result<Vec<User>, RepositoryError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let raw: Vec<Uuid> = ids.iter().map(|id| id.0).collect();
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
                 email_notifications,
                 first_logged_in_at,
                 deactivated_at,
                 version,
                 created_at,
                 updated_at
               FROM auth.users
               WHERE id = ANY($1)"#,
            &raw,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all)]
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
                 email_notifications,
                 first_logged_in_at,
                 deactivated_at,
                 version,
                 created_at,
                 updated_at
               FROM auth.users
               WHERE email = $1"#,
            email,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    #[tracing::instrument(skip_all, fields(limit = ?limit, offset = ?offset))]
    async fn list_active(
        &self,
        limit: u32,
        offset: u32,
        q: Option<&str>,
    ) -> Result<Vec<User>, RepositoryError> {
        let pattern: Option<String> = q.map(mappers::like_pattern);
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
                 email_notifications,
                 first_logged_in_at,
                 deactivated_at,
                 version,
                 created_at,
                 updated_at
               FROM auth.users
               WHERE status = 'active'
                 AND ($3::text IS NULL OR full_name ILIKE $3 OR email ILIKE $3)
               ORDER BY created_at
               LIMIT $1
               OFFSET $2"#,
            i64::from(limit),
            i64::from(offset),
            pattern,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn save(&self, user: &User, outbox: &[OutboxRecord]) -> Result<(), RepositoryError> {
        let status = SqlUserStatus::from(user.status);
        let system_role: Option<SqlSystemRole> = user.system_role.map(Into::into);
        let mut tx = self.pool.begin().await.map_err(mappers::map_pg_error)?;
        let result = sqlx::query!(
            r#"INSERT INTO auth.users AS t
                 (id, email, password_hash, full_name, avatar_storage_key, phone,
                  timezone, status, system_role, email_notifications,
                  first_logged_in_at, deactivated_at, version, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
               ON CONFLICT (id) DO UPDATE SET
                 email               = EXCLUDED.email,
                 password_hash       = EXCLUDED.password_hash,
                 full_name           = EXCLUDED.full_name,
                 avatar_storage_key  = EXCLUDED.avatar_storage_key,
                 phone               = EXCLUDED.phone,
                 timezone            = EXCLUDED.timezone,
                 status              = EXCLUDED.status,
                 system_role         = EXCLUDED.system_role,
                 email_notifications = EXCLUDED.email_notifications,
                 first_logged_in_at  = EXCLUDED.first_logged_in_at,
                 deactivated_at      = EXCLUDED.deactivated_at,
                 version             = EXCLUDED.version + 1
               WHERE t.version = EXCLUDED.version"#,
            user.id.0,
            user.email,
            user.password_hash,
            user.full_name,
            user.avatar_storage_key,
            user.phone,
            user.timezone,
            status as SqlUserStatus,
            system_role as Option<SqlSystemRole>,
            user.email_notifications,
            user.first_logged_in_at,
            user.deactivated_at,
            user.version,
            user.created_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(mappers::map_pg_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::Stale);
        }
        outbox::write(&mut tx, outbox).await?;
        tx.commit().await.map_err(mappers::map_pg_error)?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn list_avatar_keys(&self) -> Result<Vec<String>, RepositoryError> {
        let rows = sqlx::query!(
            r#"SELECT avatar_storage_key AS "avatar_storage_key!"
               FROM auth.users
               WHERE avatar_storage_key IS NOT NULL"#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(|r| r.avatar_storage_key).collect())
    }

    #[tracing::instrument(skip_all)]
    async fn list_with_system_role(&self) -> Result<Vec<User>, RepositoryError> {
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
                 email_notifications,
                 first_logged_in_at,
                 deactivated_at,
                 version,
                 created_at,
                 updated_at
               FROM auth.users
               WHERE system_role IS NOT NULL AND status = 'active'
               ORDER BY created_at"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
