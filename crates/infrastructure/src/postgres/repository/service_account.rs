use async_trait::async_trait;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    error::RepositoryError,
    ids::{ServiceAccountId, UserId},
    model::ServiceAccount,
    repository::ServiceAccountRepository,
};

use crate::postgres::{enums::SqlServiceAccountStatus, mappers};

pub struct PgServiceAccountRepo {
    pool: PgPool,
}

impl PgServiceAccountRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct ServiceAccountRow {
    id: Uuid,
    name: String,
    key_hash: Vec<u8>,
    status: SqlServiceAccountStatus,
    created_by: Uuid,
    revoked_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

impl From<ServiceAccountRow> for ServiceAccount {
    fn from(r: ServiceAccountRow) -> Self {
        Self {
            id: ServiceAccountId(r.id),
            name: r.name,
            key_hash: r.key_hash,
            status: r.status.into(),
            created_by: UserId(r.created_by),
            revoked_at: r.revoked_at,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[async_trait]
impl ServiceAccountRepository for PgServiceAccountRepo {
    #[tracing::instrument(skip_all, fields(id = ?account.id))]
    async fn create(&self, account: &ServiceAccount) -> Result<(), RepositoryError> {
        sqlx::query!(
            r#"INSERT INTO auth.service_accounts
                 (id, name, key_hash, status, created_by, revoked_at, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            account.id.0,
            account.name,
            account.key_hash,
            SqlServiceAccountStatus::from(account.status) as SqlServiceAccountStatus,
            account.created_by.0,
            account.revoked_at,
            account.created_at,
            account.updated_at,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(id = ?id))]
    async fn find_by_id(
        &self,
        id: ServiceAccountId,
    ) -> Result<Option<ServiceAccount>, RepositoryError> {
        sqlx::query_as!(
            ServiceAccountRow,
            r#"SELECT
                 id,
                 name,
                 key_hash,
                 status AS "status: SqlServiceAccountStatus",
                 created_by,
                 revoked_at,
                 created_at,
                 updated_at
               FROM auth.service_accounts
               WHERE id = $1"#,
            id.0,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    #[tracing::instrument(skip_all)]
    async fn find_active_by_key_hash(
        &self,
        key_hash: &[u8],
    ) -> Result<Option<ServiceAccount>, RepositoryError> {
        sqlx::query_as!(
            ServiceAccountRow,
            r#"SELECT
                 id,
                 name,
                 key_hash,
                 status AS "status: SqlServiceAccountStatus",
                 created_by,
                 revoked_at,
                 created_at,
                 updated_at
               FROM auth.service_accounts
               WHERE key_hash = $1 AND status = 'active'"#,
            key_hash,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(mappers::map_pg_error)
        .map(|opt| opt.map(Into::into))
    }

    #[tracing::instrument(skip_all)]
    async fn list(&self) -> Result<Vec<ServiceAccount>, RepositoryError> {
        let rows = sqlx::query_as!(
            ServiceAccountRow,
            r#"SELECT
                 id,
                 name,
                 key_hash,
                 status AS "status: SqlServiceAccountStatus",
                 created_by,
                 revoked_at,
                 created_at,
                 updated_at
               FROM auth.service_accounts
               ORDER BY created_at DESC"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(skip_all, fields(id = ?account.id))]
    async fn save(&self, account: &ServiceAccount) -> Result<(), RepositoryError> {
        let result = sqlx::query!(
            r#"UPDATE auth.service_accounts
               SET name = $2,
                   status = $3,
                   revoked_at = $4,
                   updated_at = $5
               WHERE id = $1"#,
            account.id.0,
            account.name,
            SqlServiceAccountStatus::from(account.status) as SqlServiceAccountStatus,
            account.revoked_at,
            account.updated_at,
        )
        .execute(&self.pool)
        .await
        .map_err(mappers::map_pg_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        Ok(())
    }
}
