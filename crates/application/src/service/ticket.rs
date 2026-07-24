use std::sync::Arc;

use domain::{
    ids::{TicketId, UserId},
    model::{Ticket, TicketPriority, TicketStatus},
    repository::TicketRepository,
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    commands::ticket::RaiseTicketCommand,
    error::{ConflictCode, Error, Result},
    events::{DomainEvent, EventBus},
    permissions::Permissions,
    repair::{Created, Repair, RepairJob},
    resilience,
};

pub struct TicketService {
    tickets: Arc<dyn TicketRepository>,
    perms: Arc<Permissions>,
    events: Arc<EventBus>,
    repair: Arc<Repair>,
}

impl TicketService {
    #[must_use]
    pub fn new(
        tickets: Arc<dyn TicketRepository>,
        perms: Arc<Permissions>,
        events: Arc<EventBus>,
        repair: Arc<Repair>,
    ) -> Self {
        Self {
            tickets,
            perms,
            events,
            repair,
        }
    }

    /// Raises a new IT ticket in `Open` status.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, or a repository, event, or authz-backed repository error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn raise(&self, actor: UserId, cmd: RaiseTicketCommand) -> Result<Created<Ticket>> {
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
            version: 0,
            created_at: now,
            updated_at: now,
        };
        let event = DomainEvent::TicketRaised {
            ticket_id: ticket.id,
            requester: actor,
            at: now,
            after: ticket.clone(),
        };
        self.tickets.save(&ticket, &[event.outbox_record()]).await?;
        // OpenFGA: requester + IT group + company drive the ticket viewer
        // (incl. the Director branch).
        let provisioned = self
            .repair
            .ensure(
                self.perms.grant_ticket_created(actor, ticket.id).await,
                RepairJob::SyncTicketTuples {
                    ticket_id: ticket.id,
                },
            )
            .await;
        self.events.emit(event).await;
        Ok(Created {
            entity: ticket,
            authz_pending: !provisioned,
        })
    }

    /// Triages an open ticket, setting its priority. IT-only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not an IT member, `NotFound` if the ticket does not exist, `Transition` if the ticket is not in a triageable state, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, ticket_id = ?ticket_id))]
    pub async fn triage(
        &self,
        actor: UserId,
        ticket_id: TicketId,
        priority: TicketPriority,
    ) -> Result<Ticket> {
        self.perms.require_it_member(actor).await?;
        resilience::retry_stale(|| async {
            let mut ticket = self.load(ticket_id).await?;
            let from = ticket.status;
            let now = OffsetDateTime::now_utc();
            ticket.triage(priority, now)?;
            let triaged = DomainEvent::TicketTriaged {
                ticket_id: ticket.id,
                priority,
                actor,
                at: now,
            };
            let status = status_event(&ticket, from, actor, now);
            self.tickets
                .save(&ticket, &[triaged.outbox_record(), status.outbox_record()])
                .await?;
            self.events.emit(triaged).await;
            self.events.emit(status).await;
            Ok(ticket)
        })
        .await
    }

    /// Assigns a ticket to an IT member.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not an IT member, `Conflict` if the assignee is not an IT member, `NotFound` if the ticket does not exist, `Transition` if the ticket is not in an assignable state, or a repository, event, or authz-backed repository error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, ticket_id = ?ticket_id, assignee = ?assignee))]
    pub async fn assign(
        &self,
        actor: UserId,
        ticket_id: TicketId,
        assignee: UserId,
    ) -> Result<Ticket> {
        self.perms.require_it_member(actor).await?;
        if !self.perms.is_it_member(assignee).await? {
            return Err(Error::Conflict(ConflictCode::AssigneeNotIt));
        }
        let now = OffsetDateTime::now_utc();
        let (ticket, from) = resilience::retry_stale(|| async {
            let mut ticket = self.load(ticket_id).await?;
            let from = ticket.status;
            ticket.assign(assignee, now)?;
            let assigned = DomainEvent::TicketAssigned {
                ticket_id: ticket.id,
                assignee,
                actor,
                at: now,
            };
            let status = status_event(&ticket, from, actor, now);
            self.tickets
                .save(&ticket, &[assigned.outbox_record(), status.outbox_record()])
                .await?;
            Ok((ticket, from))
        })
        .await?;
        self.repair
            .ensure(
                self.perms.grant_ticket_assignee(assignee, ticket.id).await,
                RepairJob::SyncTicketTuples {
                    ticket_id: ticket.id,
                },
            )
            .await;
        self.events
            .emit(DomainEvent::TicketAssigned {
                ticket_id: ticket.id,
                assignee,
                actor,
                at: now,
            })
            .await;
        self.events
            .emit(status_event(&ticket, from, actor, now))
            .await;
        Ok(ticket)
    }

    /// Starts work on an assigned ticket. Assignee-only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or not the assignee, `NotFound` if the ticket does not exist, `Transition` if the ticket is not in a startable state, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, ticket_id = ?ticket_id))]
    pub async fn start(&self, actor: UserId, ticket_id: TicketId) -> Result<Ticket> {
        self.perms.require_active(actor).await?;
        resilience::retry_stale(|| async {
            let mut ticket = self.load(ticket_id).await?;
            if ticket.assignee_user_id != Some(actor) {
                return Err(Error::Forbidden);
            }
            let from = ticket.status;
            let now = OffsetDateTime::now_utc();
            ticket.start(now)?;
            let status = status_event(&ticket, from, actor, now);
            self.tickets
                .save(&ticket, &[status.outbox_record()])
                .await?;
            self.events.emit(status).await;
            Ok(ticket)
        })
        .await
    }

    /// Marks an in-progress ticket as resolved. Assignee-only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or not the assignee, `NotFound` if the ticket does not exist, `Transition` if the ticket is not in a resolvable state, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, ticket_id = ?ticket_id))]
    pub async fn resolve(&self, actor: UserId, ticket_id: TicketId) -> Result<Ticket> {
        self.perms.require_active(actor).await?;
        resilience::retry_stale(|| async {
            let mut ticket = self.load(ticket_id).await?;
            if ticket.assignee_user_id != Some(actor) {
                return Err(Error::Forbidden);
            }
            let from = ticket.status;
            let now = OffsetDateTime::now_utc();
            ticket.resolve(now)?;
            let status = status_event(&ticket, from, actor, now);
            self.tickets
                .save(&ticket, &[status.outbox_record()])
                .await?;
            self.events.emit(status).await;
            Ok(ticket)
        })
        .await
    }

    /// Closes a resolved ticket. Requester-only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or not the requester, `NotFound` if the ticket does not exist, `Transition` if the ticket is not in a closable state, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, ticket_id = ?ticket_id))]
    pub async fn close(&self, actor: UserId, ticket_id: TicketId) -> Result<Ticket> {
        self.perms.require_active(actor).await?;
        resilience::retry_stale(|| async {
            let mut ticket = self.load(ticket_id).await?;
            if ticket.requester_user_id != actor {
                return Err(Error::Forbidden);
            }
            let from = ticket.status;
            let now = OffsetDateTime::now_utc();
            ticket.close(now)?;
            let status = status_event(&ticket, from, actor, now);
            self.tickets
                .save(&ticket, &[status.outbox_record()])
                .await?;
            self.events.emit(status).await;
            Ok(ticket)
        })
        .await
    }

    /// Rejects a ticket's resolution, sending it back to the assignee. Requester-only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or not the requester, `NotFound` if the ticket does not exist, `Transition` if the ticket is not in a resolved state, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, ticket_id = ?ticket_id))]
    pub async fn reject_resolution(&self, actor: UserId, ticket_id: TicketId) -> Result<Ticket> {
        self.perms.require_active(actor).await?;
        resilience::retry_stale(|| async {
            let mut ticket = self.load(ticket_id).await?;
            if ticket.requester_user_id != actor {
                return Err(Error::Forbidden);
            }
            let from = ticket.status;
            let now = OffsetDateTime::now_utc();
            ticket.reject_resolution(now)?;
            let status = status_event(&ticket, from, actor, now);
            self.tickets
                .save(&ticket, &[status.outbox_record()])
                .await?;
            self.events.emit(status).await;
            Ok(ticket)
        })
        .await
    }

    /// Reopens a closed ticket within [`Ticket::REOPEN_WINDOW`]. Requester-only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active or not the requester, `NotFound` if the ticket does not exist, `Transition` if the ticket is not closed or the reopen window has expired, or a repository or event error if the datastore or event bus is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, ticket_id = ?ticket_id))]
    pub async fn reopen(&self, actor: UserId, ticket_id: TicketId) -> Result<Ticket> {
        self.perms.require_active(actor).await?;
        resilience::retry_stale(|| async {
            let mut ticket = self.load(ticket_id).await?;
            if ticket.requester_user_id != actor {
                return Err(Error::Forbidden);
            }
            let from = ticket.status;
            let now = OffsetDateTime::now_utc();
            ticket.reopen(now)?;
            let status = status_event(&ticket, from, actor, now);
            self.tickets
                .save(&ticket, &[status.outbox_record()])
                .await?;
            self.events.emit(status).await;
            Ok(ticket)
        })
        .await
    }

    /// Lists open tickets awaiting triage, up to `limit`. IT-only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not an IT member, or a repository or authz-backed repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, limit = ?limit))]
    pub async fn list_open_for_triage(
        &self,
        actor: UserId,
        limit: u32,
        q: Option<&str>,
    ) -> Result<Vec<Ticket>> {
        self.perms.require_it_member(actor).await?;
        Ok(self.tickets.list_open_for_triage(limit, q).await?)
    }

    /// Lists tickets assigned to the actor. IT-only.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not an IT member, or a repository or authz-backed repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn list_for_assignee(&self, actor: UserId, q: Option<&str>) -> Result<Vec<Ticket>> {
        self.perms.require_it_member(actor).await?;
        Ok(self.tickets.list_for_assignee(actor, q).await?)
    }

    /// Lists tickets raised by the actor.
    ///
    /// # Errors
    /// Returns `Forbidden` if the actor is not active, or a repository or authz-backed repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor))]
    pub async fn list_for_requester(&self, actor: UserId, q: Option<&str>) -> Result<Vec<Ticket>> {
        self.perms.require_active(actor).await?;
        Ok(self.tickets.list_for_requester(actor, q).await?)
    }

    /// Finds a ticket the actor is permitted to view.
    ///
    /// # Errors
    /// Returns `NotFound` if the ticket does not exist, `Forbidden` if the actor cannot view it, or a repository or authz-backed repository error if the datastore is unavailable.
    #[tracing::instrument(skip_all, fields(actor = ?actor, ticket_id = ?ticket_id))]
    pub async fn find(&self, actor: UserId, ticket_id: TicketId) -> Result<Ticket> {
        let ticket = self.load(ticket_id).await?;
        // viewer = requester or assignee or it_member or director from company.
        self.perms.require_can_view_ticket(actor, ticket_id).await?;
        Ok(ticket)
    }

    async fn load(&self, id: TicketId) -> Result<Ticket> {
        self.tickets
            .find_by_id(id)
            .await?
            .ok_or(Error::NotFound("ticket"))
    }
}

fn status_event(
    ticket: &Ticket,
    from: TicketStatus,
    actor: UserId,
    at: OffsetDateTime,
) -> DomainEvent {
    DomainEvent::TicketStatusChanged {
        ticket_id: ticket.id,
        from,
        to: ticket.status,
        actor,
        at,
    }
}
