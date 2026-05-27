use async_trait::async_trait;

use crate::{
    error::RepositoryError,
    ids::{TicketId, UserId},
    model::Ticket,
};

#[async_trait]
pub trait TicketRepository: Send + Sync {
    async fn find_by_id(&self, id: TicketId) -> Result<Option<Ticket>, RepositoryError>;

    /// IT triage hot path — open + reopened tickets, recency-ordered.
    async fn list_open_for_triage(&self, limit: u32) -> Result<Vec<Ticket>, RepositoryError>;

    async fn list_for_assignee(&self, assignee: UserId) -> Result<Vec<Ticket>, RepositoryError>;

    async fn list_for_requester(&self, requester: UserId) -> Result<Vec<Ticket>, RepositoryError>;

    async fn save(&self, ticket: &Ticket) -> Result<(), RepositoryError>;
}
