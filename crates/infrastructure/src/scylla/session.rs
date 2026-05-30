use std::sync::Arc;

// `::scylla` names the driver crate explicitly; a bare `scylla` here would be
// ambiguous with this crate's own `scylla` module.
use ::scylla::client::{session::Session, session_builder::SessionBuilder};

/// Errors raised while building the shared Scylla session.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("no scylla hosts configured")]
    NoHosts,
    #[error("failed to connect: {0}")]
    Connect(String),
}

/// Builds a shared Scylla session from contact points and selects the keyspace.
///
/// `hosts` are `host:port` contact points (parsed from `SCYLLA_HOSTS`); the
/// driver discovers the rest of the cluster from them. The session is wrapped
/// in `Arc` because every chat repository shares one session and clones the
/// handle cheaply. Mirrors `postgres::build_pool` so the composition root
/// constructs both backends the same way.
pub async fn build_session(hosts: &[String], keyspace: &str) -> Result<Arc<Session>, SessionError> {
    if hosts.is_empty() {
        return Err(SessionError::NoHosts);
    }
    let session = SessionBuilder::new()
        .known_nodes(hosts)
        .build()
        .await
        .map_err(|e| SessionError::Connect(e.to_string()))?;
    session
        .use_keyspace(keyspace, false)
        .await
        .map_err(|e| SessionError::Connect(e.to_string()))?;
    Ok(Arc::new(session))
}
