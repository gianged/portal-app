use async_trait::async_trait;
use redis::{Client, aio::ConnectionManager};

use domain::{
    error::SpoolError,
    ports::spool::{Spool, SpoolEntry, SpoolId},
};

/// Namespaced key holding one spool's backlog list.
fn spool_key(name: &str) -> String {
    format!("portal:spool:{name}")
}

/// Redis-list-backed durable spool: `RPUSH` to append, `LRANGE` to peek the head,
/// `LTRIM` to drop acked entries. Redis is the chosen backend because it stays up
/// while the Postgres/Scylla outage being guarded against is in progress.
///
/// Assumes a single drainer: [`Spool::drain`] returns head-relative ids and
/// [`Spool::ack`] trims that many entries off the head, which is only correct
/// when one consumer drains and acks a contiguous head prefix.
#[derive(Clone)]
pub struct RedisSpool {
    conn: ConnectionManager,
    key: String,
}

impl RedisSpool {
    pub async fn new(url: &str, name: &str) -> Result<Self, SpoolError> {
        let client = Client::open(url).map_err(backend)?;
        let conn = ConnectionManager::new(client).await.map_err(backend)?;
        Ok(Self {
            conn,
            key: spool_key(name),
        })
    }
}

#[async_trait]
impl Spool for RedisSpool {
    async fn push(&self, batch: &[u8]) -> Result<(), SpoolError> {
        let mut conn = self.conn.clone();
        redis::cmd("RPUSH")
            .arg(&self.key)
            .arg(batch)
            .query_async::<i64>(&mut conn)
            .await
            .map_err(backend)?;
        Ok(())
    }

    async fn drain(&self, max: usize) -> Result<Vec<SpoolEntry>, SpoolError> {
        if max == 0 {
            return Ok(Vec::new());
        }
        let mut conn = self.conn.clone();
        let stop = isize::try_from(max - 1).unwrap_or(isize::MAX);
        let payloads: Vec<Vec<u8>> = redis::cmd("LRANGE")
            .arg(&self.key)
            .arg(0)
            .arg(stop)
            .query_async(&mut conn)
            .await
            .map_err(backend)?;
        let entries = payloads
            .into_iter()
            .enumerate()
            .map(|(i, payload)| SpoolEntry {
                id: SpoolId(i as u64),
                payload,
            })
            .collect();
        Ok(entries)
    }

    async fn ack(&self, ids: &[SpoolId]) -> Result<(), SpoolError> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.clone();
        // Keep everything from index len..end, dropping the acked head prefix.
        let keep_from = isize::try_from(ids.len()).unwrap_or(isize::MAX);
        redis::cmd("LTRIM")
            .arg(&self.key)
            .arg(keep_from)
            .arg(-1)
            .query_async::<()>(&mut conn)
            .await
            .map_err(backend)?;
        Ok(())
    }
}

fn backend<E: std::fmt::Display>(e: E) -> SpoolError {
    SpoolError::Backend(e.to_string())
}
