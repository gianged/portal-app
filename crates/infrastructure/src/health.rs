//! Per-backend liveness probes implementing [`HealthCheck`]. Each is the cheapest
//! round-trip that proves the backend answers, under a short timeout so a hung
//! backend reports `Down` quickly.

use std::{fmt::Display, sync::Arc, time::Duration};

use async_trait::async_trait;
use proto::{
    tonic::transport::{Channel, Endpoint},
    tonic_health::pb::{
        HealthCheckRequest, health_check_response::ServingStatus, health_client::HealthClient,
    },
};
use redis::aio::ConnectionManager;
use reqwest::StatusCode;
// `::scylla` names the driver crate, not this crate's own `scylla` module.
use ::scylla::client::session::Session;
use sqlx::PgPool;
use tokio::time;

use domain::{error::HealthError, health::BackendId, ports::health::HealthCheck};

/// Upper bound on any single probe; a hung backend reports `Down` after this.
const PING_TIMEOUT: Duration = Duration::from_secs(2);

/// Runs `fut` under [`PING_TIMEOUT`], mapping an elapsed deadline to `Timeout`.
async fn with_timeout<F, T>(fut: F) -> Result<T, HealthError>
where
    F: Future<Output = Result<T, HealthError>>,
{
    time::timeout(PING_TIMEOUT, fut)
        .await
        .unwrap_or(Err(HealthError::Timeout))
}

fn backend<E: Display>(e: E) -> HealthError {
    HealthError::Backend(e.to_string())
}

/// Postgres probe: `SELECT 1` on the shared pool.
pub struct PgHealthCheck {
    pool: PgPool,
}

impl PgHealthCheck {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for PgHealthCheck {
    fn backend(&self) -> BackendId {
        BackendId::Postgres
    }

    #[tracing::instrument(skip_all)]
    async fn ping(&self) -> Result<(), HealthError> {
        with_timeout(async {
            // A constant liveness ping needs no compile-time check; the typed
            // query macros are reserved for the repositories that map rows.
            sqlx::query("SELECT 1")
                .execute(&self.pool)
                .await
                .map_err(backend)?;
            Ok(())
        })
        .await
    }
}

/// Scylla probe: a trivial read against the always-present `system.local`.
pub struct ScyllaHealthCheck {
    session: Arc<Session>,
}

impl ScyllaHealthCheck {
    #[must_use]
    pub fn new(session: Arc<Session>) -> Self {
        Self { session }
    }
}

#[async_trait]
impl HealthCheck for ScyllaHealthCheck {
    fn backend(&self) -> BackendId {
        BackendId::Scylla
    }

    #[tracing::instrument(skip_all)]
    async fn ping(&self) -> Result<(), HealthError> {
        with_timeout(async {
            self.session
                .query_unpaged("SELECT now() FROM system.local", &[])
                .await
                .map_err(backend)?;
            Ok(())
        })
        .await
    }
}

/// Redis probe: `PING` over a dedicated multiplexed connection.
pub struct RedisHealthCheck {
    conn: ConnectionManager,
}

impl RedisHealthCheck {
    /// Opens a dedicated connection for probing; kept separate from the app
    /// connections so a saturated app pool doesn't mask a healthy server.
    pub async fn new(url: &str) -> Result<Self, HealthError> {
        let conn = crate::redis::connect_manager(url).await.map_err(backend)?;
        Ok(Self { conn })
    }
}

#[async_trait]
impl HealthCheck for RedisHealthCheck {
    fn backend(&self) -> BackendId {
        BackendId::Redis
    }

    #[tracing::instrument(skip_all)]
    async fn ping(&self) -> Result<(), HealthError> {
        let mut conn = self.conn.clone();
        with_timeout(async move {
            let pong: String = redis::cmd("PING")
                .query_async(&mut conn)
                .await
                .map_err(backend)?;
            if pong == "PONG" {
                Ok(())
            } else {
                Err(HealthError::Backend(format!(
                    "unexpected PING reply: {pong}"
                )))
            }
        })
        .await
    }
}

/// `OpenFGA` probe: `GET /healthz`.
pub struct OpenFgaHealthCheck {
    http: reqwest::Client,
    healthz_url: String,
    bearer_token: Option<String>,
}

impl OpenFgaHealthCheck {
    pub fn new(endpoint: &str, bearer_token: Option<String>) -> Result<Self, HealthError> {
        let http = reqwest::Client::builder()
            .tls_backend_rustls()
            .build()
            .map_err(backend)?;
        Ok(Self {
            http,
            healthz_url: format!("{}/healthz", endpoint.trim_end_matches('/')),
            bearer_token,
        })
    }
}

#[async_trait]
impl HealthCheck for OpenFgaHealthCheck {
    fn backend(&self) -> BackendId {
        BackendId::OpenFga
    }

    #[tracing::instrument(skip_all)]
    async fn ping(&self) -> Result<(), HealthError> {
        with_timeout(async {
            let mut req = self.http.get(&self.healthz_url);
            if let Some(token) = &self.bearer_token {
                req = req.bearer_auth(token);
            }
            let status = req.send().await.map_err(backend)?.status();
            if status == StatusCode::OK {
                Ok(())
            } else {
                Err(HealthError::Backend(format!("healthz returned {status}")))
            }
        })
        .await
    }
}

/// Workers-gRPC probe: standard `grpc.health.v1/Check` against the workers'
/// ingest plane. Non-gating in readiness — dispatch falls back to apalis.
pub struct WorkersGrpcHealthCheck {
    channel: Channel,
}

impl WorkersGrpcHealthCheck {
    pub fn new(url: &str) -> Result<Self, HealthError> {
        let channel = Endpoint::from_shared(url.to_owned())
            .map_err(|e| HealthError::Backend(e.to_string()))?
            .connect_timeout(PING_TIMEOUT)
            .timeout(PING_TIMEOUT)
            .connect_lazy();
        Ok(Self { channel })
    }
}

#[async_trait]
impl HealthCheck for WorkersGrpcHealthCheck {
    fn backend(&self) -> BackendId {
        BackendId::WorkersGrpc
    }

    #[tracing::instrument(skip_all)]
    async fn ping(&self) -> Result<(), HealthError> {
        with_timeout(async {
            let mut client = HealthClient::new(self.channel.clone());
            let response = client
                .check(HealthCheckRequest {
                    service: String::new(),
                })
                .await
                .map_err(backend)?;
            if response.get_ref().status() == ServingStatus::Serving {
                Ok(())
            } else {
                Err(HealthError::Backend("workers grpc not serving".to_owned()))
            }
        })
        .await
    }
}
