use std::time::Duration;

use redis::{
    Client, RedisError,
    aio::{ConnectionManager, ConnectionManagerConfig},
};

/// Shared manager constructor: bounded connect/response times so a stalled
/// Redis fails fast into the caller's error path instead of hanging requests.
pub(crate) async fn connect_manager(url: &str) -> Result<ConnectionManager, RedisError> {
    let client = Client::open(url)?;
    let config = ConnectionManagerConfig::new()
        .set_connection_timeout(Some(Duration::from_secs(2)))
        .set_response_timeout(Some(Duration::from_secs(2)));
    ConnectionManager::new_with_config(client, config).await
}
