use async_trait::async_trait;
use time::OffsetDateTime;

use crate::{
    error::RepositoryError,
    ids::{TicketId, UserId},
    model::Ticket,
    repository::OutboxRecord,
};

/// `q` parameters are case-insensitive substring filters on the ticket title;
/// `None` skips the filter.
#[async_trait]
pub trait TicketRepository: Send + Sync {
    async fn find_by_id(&self, id: TicketId) -> Result<Option<Ticket>, RepositoryError>;

    /// IT triage hot path: open + reopened tickets, recency-ordered.
    async fn list_open_for_triage(
        &self,
        limit: u32,
        q: Option<&str>,
    ) -> Result<Vec<Ticket>, RepositoryError>;

    async fn list_for_assignee(
        &self,
        assignee: UserId,
        q: Option<&str>,
    ) -> Result<Vec<Ticket>, RepositoryError>;

    async fn list_for_requester(
        &self,
        requester: UserId,
        q: Option<&str>,
    ) -> Result<Vec<Ticket>, RepositoryError>;

    /// Tickets still in `Resolved` whose `resolved_at` is at or before `cutoff`;
    /// the auto-close sweep's work list, oldest first.
    async fn list_resolved_before(
        &self,
        cutoff: OffsetDateTime,
        limit: u32,
    ) -> Result<Vec<Ticket>, RepositoryError>;

    /// `outbox` rows commit in the same transaction as the entity write, so an
    /// audited event cannot be lost between commit and projection.
    async fn save(&self, ticket: &Ticket, outbox: &[OutboxRecord]) -> Result<(), RepositoryError>;
}
