use std::time::Duration;

use async_trait::async_trait;
use proto::{
    auth::AttachToken,
    internal::v1::{EnqueueRequest, jobs_client::JobsClient},
    tonic::{
        Request,
        service::interceptor::InterceptedService,
        transport::{Channel, Endpoint},
    },
};

use domain::{error::JobError, ports::job_queue::JobQueue};

use crate::telemetry;

/// [`JobQueue`] over the workers' internal `Jobs` gRPC service. Lazily
/// connected: the channel dials on first use and reconnects on its own. No
/// retry or breaker here; the dispatch chain in `application` owns that policy.
pub struct GrpcJobQueue {
    client: JobsClient<InterceptedService<Channel, AttachToken>>,
}

impl GrpcJobQueue {
    pub fn new(url: &str, token: &str) -> Result<Self, JobError> {
        let channel = Endpoint::from_shared(url.to_owned())
            .map_err(|e| JobError::Backend(e.to_string()))?
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(2))
            .connect_lazy();
        let attach = AttachToken::new(token).map_err(|e| JobError::Backend(e.to_string()))?;
        Ok(Self {
            client: JobsClient::with_interceptor(channel, attach),
        })
    }
}

#[async_trait]
impl JobQueue for GrpcJobQueue {
    #[tracing::instrument(skip_all)]
    async fn enqueue(&self, queue: &str, payload: &[u8]) -> Result<(), JobError> {
        let mut client = self.client.clone();
        let request = Request::new(EnqueueRequest {
            queue: queue.to_owned(),
            payload: payload.to_vec(),
            traceparent: telemetry::current_traceparent().unwrap_or_default(),
        });
        client
            .enqueue(request)
            .await
            .map_err(|e| JobError::Backend(e.to_string()))?;
        Ok(())
    }
}
