use std::time::Duration;

use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};

#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("invalid connection options: {0}")]
    Configure(String),
    #[error("failed to connect: {0}")]
    Connect(String),
}

pub async fn build_pool(database_url: &str, max_connections: u32) -> Result<PgPool, PoolError> {
    let opts: PgConnectOptions = database_url
        .parse()
        .map_err(|e: sqlx::Error| PoolError::Configure(e.to_string()))?;

    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_mins(10))
        .max_lifetime(Duration::from_mins(30))
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SET statement_timeout = '5s'")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with(opts)
        .await
        .map_err(|e| PoolError::Connect(e.to_string()))
}
