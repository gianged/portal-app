use std::sync::Arc;

use domain::{
    ids::{TicketId, UserId},
    model::{Ticket, TicketPriority, TicketStatus},
    repository::TicketRepository,
};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::{
    commands::ticket::RaiseTicketCommand,
    error::{Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
};

/// A closed ticket can be reopened within this window. Past it, the requester
/// must raise a new ticket.
const REOPEN_WINDOW: Duration = Duration::days(7);

pub struct TicketService {
    tickets: Arc<dyn TicketRepository>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
}

impl TicketService {
    #[must_use]
    pub fn new(
        tickets: Arc<dyn TicketRepository>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
    ) -> Self {
        Self {
            tickets,
            perms,
            events,
        }
    }

    pub async fn raise(&self, actor: UserId, cmd: RaiseTicketCommand) -> Result<Ticket> {
        self.perms.require_active(actor).await?;
        let now = OffsetDateTime::now_utc();
        let ticket = Ticket {
            id: TicketId(Uuid::now_v7()),
            requester_user_id: actor,
            assignee_user_id: None,
            title: cmd.title,
            description: cmd.description,
            status: TicketStatus::Open,
            priority: None,
            category: cmd.category,
            triaged_at: None,
            resolved_at: None,
            closed_at: None,
            created_at: now,
            updated_at: now,
        };
        self.tickets.save(&ticket).await?;
        // OpenFGA: requester + IT group + company drive the ticket viewer
        // (incl. the Director branch).
        self.perms.grant_ticket_created(actor, ticket.id).await?;
        self.events
            .emit(DomainEvent::TicketRaised {
                ticket_id: ticket.id,
                requester: actor,
                at: now,
                after: ticket.clone(),
            })
            .await?;
        Ok(ticket)
    }

    pub async fn triage(
        &self,
        actor: UserId,
        ticket_id: TicketId,
        priority: TicketPriority,
    ) -> Result<Ticket> {
        self.perms.require_it_member(actor).await?;
        let mut ticket = self.load(ticket_id).await?;
        let from = ticket.status;
        let now = OffsetDateTime::now_utc();
        ticket.triage(priority, now)?;
        self.tickets.save(&ticket).await?;
        self.events
            .emit(DomainEvent::TicketTriaged {
                ticket_id: ticket.id,
                priority,
                actor,
                at: now,
            })
            .await?;
        self.emit_status(actor, &ticket, from, now).await?;
        Ok(ticket)
    }

    pub async fn assign(
        &self,
        actor: UserId,
        ticket_id: TicketId,
        assignee: UserId,
    ) -> Result<Ticket> {
        self.perms.require_it_member(actor).await?;
        if !self.perms.is_it_member(assignee).await? {
            return Err(Error::Conflict("assignee_not_it".into()));
        }
        let mut ticket = self.load(ticket_id).await?;
        let from = ticket.status;
        let now = OffsetDateTime::now_utc();
        ticket.assign(assignee, now)?;
        self.tickets.save(&ticket).await?;
        self.perms
            .grant_ticket_assignee(assignee, ticket.id)
            .await?;
        self.events
            .emit(DomainEvent::TicketAssigned {
                ticket_id: ticket.id,
                assignee,
                actor,
                at: now,
            })
            .await?;
        self.emit_status(actor, &ticket, from, now).await?;
        Ok(ticket)
    }

    pub async fn start(&self, actor: UserId, ticket_id: TicketId) -> Result<Ticket> {
        let mut ticket = self.load(ticket_id).await?;
        if ticket.assignee_user_id != Some(actor) {
            return Err(Error::Forbidden);
        }
        let from = ticket.status;
        let now = OffsetDateTime::now_utc();
        ticket.start(now)?;
        self.tickets.save(&ticket).await?;
        self.emit_status(actor, &ticket, from, now).await?;
        Ok(ticket)
    }

    pub async fn resolve(&self, actor: UserId, ticket_id: TicketId) -> Result<Ticket> {
        let mut ticket = self.load(ticket_id).await?;
        if ticket.assignee_user_id != Some(actor) {
            return Err(Error::Forbidden);
        }
        let from = ticket.status;
        let now = OffsetDateTime::now_utc();
        ticket.resolve(now)?;
        self.tickets.save(&ticket).await?;
        self.emit_status(actor, &ticket, from, now).await?;
        Ok(ticket)
    }

    pub async fn close(&self, actor: UserId, ticket_id: TicketId) -> Result<Ticket> {
        let mut ticket = self.load(ticket_id).await?;
        if ticket.requester_user_id != actor {
            return Err(Error::Forbidden);
        }
        let from = ticket.status;
        let now = OffsetDateTime::now_utc();
        ticket.close(now)?;
        self.tickets.save(&ticket).await?;
        self.emit_status(actor, &ticket, from, now).await?;
        Ok(ticket)
    }

    pub async fn reject_resolution(&self, actor: UserId, ticket_id: TicketId) -> Result<Ticket> {
        let mut ticket = self.load(ticket_id).await?;
        if ticket.requester_user_id != actor {
            return Err(Error::Forbidden);
        }
        let from = ticket.status;
        let now = OffsetDateTime::now_utc();
        ticket.reject_resolution(now)?;
        self.tickets.save(&ticket).await?;
        self.emit_status(actor, &ticket, from, now).await?;
        Ok(ticket)
    }

    pub async fn reopen(&self, actor: UserId, ticket_id: TicketId) -> Result<Ticket> {
        let mut ticket = self.load(ticket_id).await?;
        if ticket.requester_user_id != actor {
            return Err(Error::Forbidden);
        }
        let closed_at = ticket
            .closed_at
            .ok_or_else(|| Error::Conflict("ticket_not_closed".into()))?;
        let now = OffsetDateTime::now_utc();
        if now - closed_at > REOPEN_WINDOW {
            return Err(Error::Conflict("reopen_window_expired".into()));
        }
        let from = ticket.status;
        ticket.reopen(now)?;
        self.tickets.save(&ticket).await?;
        self.emit_status(actor, &ticket, from, now).await?;
        Ok(ticket)
    }

    pub async fn list_open_for_triage(&self, actor: UserId, limit: u32) -> Result<Vec<Ticket>> {
        self.perms.require_it_member(actor).await?;
        Ok(self.tickets.list_open_for_triage(limit).await?)
    }

    pub async fn list_for_assignee(&self, actor: UserId) -> Result<Vec<Ticket>> {
        self.perms.require_it_member(actor).await?;
        Ok(self.tickets.list_for_assignee(actor).await?)
    }

    pub async fn list_for_requester(&self, actor: UserId) -> Result<Vec<Ticket>> {
        self.perms.require_active(actor).await?;
        Ok(self.tickets.list_for_requester(actor).await?)
    }

    pub async fn find(&self, actor: UserId, ticket_id: TicketId) -> Result<Ticket> {
        let ticket = self.load(ticket_id).await?;
        // viewer = requester or assignee or it_member or director from company.
        self.perms.require_can_view_ticket(actor, ticket_id).await?;
        Ok(ticket)
    }

    async fn emit_status(
        &self,
        actor: UserId,
        ticket: &Ticket,
        from: TicketStatus,
        at: OffsetDateTime,
    ) -> Result<()> {
        self.events
            .emit(DomainEvent::TicketStatusChanged {
                ticket_id: ticket.id,
                from,
                to: ticket.status,
                actor,
                at,
            })
            .await
    }

    async fn load(&self, id: TicketId) -> Result<Ticket> {
        self.tickets
            .find_by_id(id)
            .await?
            .ok_or(Error::NotFound("ticket"))
    }
}
