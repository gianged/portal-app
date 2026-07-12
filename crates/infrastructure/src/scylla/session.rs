use std::{sync::Arc, time::Duration};

// `::scylla` names the driver crate, not this crate's own `scylla` module.
use ::scylla::client::{session::Session, session_builder::SessionBuilder};

/// Bounds each contact-point connect so a dead host fails fast instead of
/// hanging on the OS TCP timeout.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Errors raised while building the shared Scylla session.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("no scylla hosts configured")]
    NoHosts,
    #[error("failed to connect: {0}")]
    Connect(String),
}

/// Builds a shared Scylla session from contact points and selects the keyspace.
pub async fn build_session(hosts: &[String], keyspace: &str) -> Result<Arc<Session>, SessionError> {
    if hosts.is_empty() {
        return Err(SessionError::NoHosts);
    }
    let session = Box::pin(
        SessionBuilder::new()
            .known_nodes(hosts)
            .connection_timeout(CONNECT_TIMEOUT)
            .build(),
    )
    .await
    .map_err(|e| SessionError::Connect(e.to_string()))?;
    session
        .use_keyspace(keyspace, false)
        .await
        .map_err(|e| SessionError::Connect(e.to_string()))?;
    Ok(Arc::new(session))
}
