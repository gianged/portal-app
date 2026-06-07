//! Domain <-> wire projections for IT tickets.

use application::commands::ticket::RaiseTicketCommand;
use domain::model;
use shared::dto::{
    common::UserSummaryDto,
    ticket::{
        RaiseTicketRequest, TicketCategory as WireTicketCategory, TicketDto,
        TicketPriority as WireTicketPriority, TicketStatus as WireTicketStatus,
    },
};

use super::ticket_id;

// --- enums ---

#[must_use]
pub fn ticket_status_dto(status: model::TicketStatus) -> WireTicketStatus {
    match status {
        model::TicketStatus::Open => WireTicketStatus::Open,
        model::TicketStatus::Triaged => WireTicketStatus::Triaged,
        model::TicketStatus::Assigned => WireTicketStatus::Assigned,
        model::TicketStatus::InProgress => WireTicketStatus::InProgress,
        model::TicketStatus::Resolved => WireTicketStatus::Resolved,
        model::TicketStatus::Closed => WireTicketStatus::Closed,
        model::TicketStatus::Reopened => WireTicketStatus::Reopened,
    }
}

#[must_use]
pub fn ticket_priority_dto(priority: model::TicketPriority) -> WireTicketPriority {
    match priority {
        model::TicketPriority::Low => WireTicketPriority::Low,
        model::TicketPriority::Normal => WireTicketPriority::Normal,
        model::TicketPriority::High => WireTicketPriority::High,
        model::TicketPriority::Urgent => WireTicketPriority::Urgent,
    }
}

#[must_use]
pub fn ticket_priority_domain(priority: WireTicketPriority) -> model::TicketPriority {
    match priority {
        WireTicketPriority::Low => model::TicketPriority::Low,
        WireTicketPriority::Normal => model::TicketPriority::Normal,
        WireTicketPriority::High => model::TicketPriority::High,
        WireTicketPriority::Urgent => model::TicketPriority::Urgent,
    }
}

#[must_use]
pub fn ticket_category_dto(category: model::TicketCategory) -> WireTicketCategory {
    match category {
        model::TicketCategory::Hardware => WireTicketCategory::Hardware,
        model::TicketCategory::Software => WireTicketCategory::Software,
        model::TicketCategory::Access => WireTicketCategory::Access,
        model::TicketCategory::Other => WireTicketCategory::Other,
    }
}

#[must_use]
pub fn ticket_category_domain(category: WireTicketCategory) -> model::TicketCategory {
    match category {
        WireTicketCategory::Hardware => model::TicketCategory::Hardware,
        WireTicketCategory::Software => model::TicketCategory::Software,
        WireTicketCategory::Access => model::TicketCategory::Access,
        WireTicketCategory::Other => model::TicketCategory::Other,
    }
}

// --- views ---

/// Builds a `TicketDto` from a ticket plus its already-resolved user summaries.
#[must_use]
pub fn ticket_dto(
    ticket: &model::Ticket,
    requester: UserSummaryDto,
    assignee: Option<UserSummaryDto>,
) -> TicketDto {
    TicketDto {
        id: ticket_id(ticket.id),
        requester,
        assignee,
        title: ticket.title.clone(),
        description: ticket.description.clone(),
        status: ticket_status_dto(ticket.status),
        priority: ticket.priority.map(ticket_priority_dto),
        category: ticket_category_dto(ticket.category),
        triaged_at: ticket.triaged_at,
        resolved_at: ticket.resolved_at,
        closed_at: ticket.closed_at,
        created_at: ticket.created_at,
        updated_at: ticket.updated_at,
    }
}

// --- commands ---

#[must_use]
pub fn raise_ticket_command(req: RaiseTicketRequest) -> RaiseTicketCommand {
    RaiseTicketCommand {
        title: req.title,
        description: req.description,
        category: ticket_category_domain(req.category),
    }
}
