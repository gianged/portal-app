use async_trait::async_trait;

use crate::error::SpoolError;

/// Opaque handle for one spooled entry, returned by [`Spool::drain`] and passed
/// back to [`Spool::ack`]. The adapter assigns and interprets it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpoolId(pub u64);

/// One drained backlog entry: its id plus the opaque payload bytes that were
/// pushed (a serialised batch).
#[derive(Debug, Clone)]
pub struct SpoolEntry {
    pub id: SpoolId,
    pub payload: Vec<u8>,
}

/// Durable backlog for work that couldn't reach a downed backend. The chat
/// write-behind path pushes serialised batches here when Scylla is down; the
/// drainer drains, replays, and acks once the backend recovers.
///
/// Entries are acked only after a successful replay, so a crash mid-replay
/// re-delivers them (at-least-once). Replays must therefore be idempotent.
#[async_trait]
pub trait Spool: Send + Sync {
    /// Append one serialised batch to the tail of the backlog.
    async fn push(&self, batch: &[u8]) -> Result<(), SpoolError>;

    /// Peek up to `max` entries from the head without removing them. Re-draining
    /// before an ack returns the same head entries.
    async fn drain(&self, max: usize) -> Result<Vec<SpoolEntry>, SpoolError>;

    /// Remove the acked entries from the head of the backlog. Ids must be a
    /// contiguous head prefix of the last [`Spool::drain`].
    async fn ack(&self, ids: &[SpoolId]) -> Result<(), SpoolError>;
}
