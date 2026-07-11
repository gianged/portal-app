//! Internal gRPC ingest: the server enqueues durable jobs here and the handler
//! pushes them into the same apalis storages this process consumes, so queue
//! durability and retry semantics stay in apalis.

use std::net::SocketAddr;

use apalis_redis::RedisStorage;

use domain::ports::job_queue::{QUEUE_AUDIT, QUEUE_EMAILS, QUEUE_NOTIFICATIONS};
use infrastructure::jobs::{AuditEnvelope, EmailEnvelope, Envelope, NotificationEnvelope};
use proto::{
    auth::RequireToken,
    internal::v1::{
        EnqueueRequest, EnqueueResponse,
        jobs_server::{Jobs, JobsServer},
    },
    tonic::{self, Request, Response, Status, transport::Server},
    tonic_health::server,
};

use crate::job_spool;

/// Jobs service backed by the three local apalis storages.
pub struct GrpcJobs {
    notifications: RedisStorage<NotificationEnvelope>,
    audit: RedisStorage<AuditEnvelope>,
    emails: RedisStorage<EmailEnvelope>,
}

impl GrpcJobs {
    /// Wraps the storages the enqueue handler pushes into.
    pub fn new(
        notifications: RedisStorage<NotificationEnvelope>,
        audit: RedisStorage<AuditEnvelope>,
        emails: RedisStorage<EmailEnvelope>,
    ) -> Self {
        Self {
            notifications,
            audit,
            emails,
        }
    }
}

#[tonic::async_trait]
impl Jobs for GrpcJobs {
    async fn enqueue(
        &self,
        request: Request<EnqueueRequest>,
    ) -> Result<Response<EnqueueResponse>, Status> {
        let EnqueueRequest {
            queue,
            payload,
            traceparent,
        } = request.into_inner();
        let traceparent = (!traceparent.is_empty()).then_some(traceparent);
        match queue.as_str() {
            QUEUE_NOTIFICATIONS => {
                push(
                    self.notifications.clone(),
                    NotificationEnvelope::new(payload, traceparent),
                )
                .await?;
            }
            QUEUE_AUDIT => {
                push(self.audit.clone(), AuditEnvelope::new(payload, traceparent)).await?;
            }
            QUEUE_EMAILS => {
                push(
                    self.emails.clone(),
                    EmailEnvelope::new(payload, traceparent),
                )
                .await?;
            }
            other => return Err(Status::invalid_argument(format!("unknown queue: {other}"))),
        }
        Ok(Response::new(EnqueueResponse {}))
    }
}

async fn push<E: Envelope>(storage: RedisStorage<E>, envelope: E) -> Result<(), Status> {
    job_spool::push(storage, envelope)
        .await
        .map_err(|e| Status::unavailable(e.to_string()))
}

/// Serves `Jobs` (token-gated) plus the standard gRPC health service (open, so
/// the server's probe needs no credentials). Resolves on the shutdown signal.
pub async fn serve(jobs: GrpcJobs, addr: SocketAddr, token: &str) -> anyhow::Result<()> {
    let (health_reporter, health_service) = server::health_reporter();
    health_reporter.set_serving::<JobsServer<GrpcJobs>>().await;
    tracing::info!(%addr, "workers grpc listening");
    Server::builder()
        .add_service(health_service)
        .add_service(JobsServer::with_interceptor(jobs, RequireToken::new(token)))
        .serve_with_shutdown(addr, crate::wait_for_shutdown())
        .await?;
    Ok(())
}
