//! Internal gRPC plane: the trusted `Query` service served on a second
//! listener, gated by the shared internal token.

pub mod query;

use std::net::SocketAddr;

use proto::{
    auth::RequireToken, internal::v1::query_server::QueryServer, tonic::transport::Server,
};

use crate::grpc::query::QueryService;

/// Everything `run` needs to stand up the internal listener; built by
/// `app::build` alongside the HTTP router.
pub struct GrpcPlane {
    addr: SocketAddr,
    token: String,
    query: QueryService,
}

impl GrpcPlane {
    #[must_use]
    pub fn new(addr: SocketAddr, token: String, query: QueryService) -> Self {
        Self { addr, token, query }
    }

    /// Serves the `Query` plane until `shutdown` resolves.
    pub async fn serve<F>(self, shutdown: F) -> anyhow::Result<()>
    where
        F: Future<Output = ()> + Send,
    {
        tracing::info!(addr = %self.addr, "internal grpc listening");
        Server::builder()
            .add_service(QueryServer::with_interceptor(
                self.query,
                RequireToken::new(&self.token),
            ))
            .serve_with_shutdown(self.addr, shutdown)
            .await?;
        Ok(())
    }
}
