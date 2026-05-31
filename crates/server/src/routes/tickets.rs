//! IT ticket endpoints. Listing is scope-filtered via a query param to avoid a
//! static/param routing conflict with `/tickets/{id}`.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::Deserialize;
use uuid::Uuid;

use domain::{
    ids::{TicketId, UserId},
    model::Ticket,
};
use shared::dto::ticket::{
    AssignTicketRequest, RaiseTicketRequest, TicketDto, TriageTicketRequest,
};
use shared::validation::ticket::{validate_ticket_description, validate_ticket_title};

use crate::{app::AppState, dto, error::AppError, extractors::auth_user::AuthUser, resolve};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tickets", post(raise).get(list))
        .route("/tickets/{id}", get(get_one))
        .route("/tickets/{id}/triage", post(triage))
        .route("/tickets/{id}/assign", post(assign))
        .route("/tickets/{id}/start", post(start))
        .route("/tickets/{id}/resolve", post(resolve_ticket))
        .route("/tickets/{id}/reject", post(reject))
        .route("/tickets/{id}/close", post(close))
        .route("/tickets/{id}/reopen", post(reopen))
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Scope {
    /// IT triage queue (open + reopened).
    Triage,
    /// Tickets assigned to the caller (IT staff).
    Assigned,
    /// Tickets the caller raised.
    Mine,
}

#[derive(Deserialize)]
struct ListQuery {
    scope: Option<Scope>,
    limit: Option<u32>,
}

async fn raise(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RaiseTicketRequest>,
) -> Result<Json<TicketDto>, AppError> {
    validate_ticket_title(&body.title).map_err(|e| AppError::Validation(e.to_string()))?;
    validate_ticket_description(&body.description)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let ticket = state
        .ticket
        .raise(auth.user_id, dto::raise_ticket_command(body))
        .await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<TicketDto>>, AppError> {
    let tickets = match q.scope.unwrap_or(Scope::Mine) {
        Scope::Triage => {
            let limit = q.limit.unwrap_or(50).clamp(1, 200);
            state
                .ticket
                .list_open_for_triage(auth.user_id, limit)
                .await?
        }
        Scope::Assigned => state.ticket.list_for_assignee(auth.user_id).await?,
        Scope::Mine => state.ticket.list_for_requester(auth.user_id).await?,
    };
    Ok(Json(many(&state, tickets).await?))
}

async fn get_one(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state.ticket.find(auth.user_id, TicketId(id)).await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn triage(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<TriageTicketRequest>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state
        .ticket
        .triage(
            auth.user_id,
            TicketId(id),
            dto::ticket_priority_domain(body.priority),
        )
        .await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn assign(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AssignTicketRequest>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state
        .ticket
        .assign(auth.user_id, TicketId(id), UserId(body.assignee_user_id.0))
        .await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn start(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state.ticket.start(auth.user_id, TicketId(id)).await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn resolve_ticket(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state.ticket.resolve(auth.user_id, TicketId(id)).await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn reject(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state
        .ticket
        .reject_resolution(auth.user_id, TicketId(id))
        .await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn close(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state.ticket.close(auth.user_id, TicketId(id)).await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn reopen(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state.ticket.reopen(auth.user_id, TicketId(id)).await?;
    Ok(Json(single(&state, &ticket).await?))
}

/// Resolves one ticket's requester + assignee summaries.
async fn single(state: &AppState, ticket: &Ticket) -> Result<TicketDto, AppError> {
    let requester =
        resolve::user_summary(&state.user, &state.group, ticket.requester_user_id).await?;
    let assignee =
        resolve::opt_user_summary(&state.user, &state.group, ticket.assignee_user_id).await?;
    Ok(dto::ticket_dto(ticket, requester, assignee))
}

/// Resolves a batch of tickets, deduplicating user lookups.
async fn many(state: &AppState, tickets: Vec<Ticket>) -> Result<Vec<TicketDto>, AppError> {
    let mut ids: Vec<UserId> = Vec::with_capacity(tickets.len() * 2);
    for t in &tickets {
        ids.push(t.requester_user_id);
        if let Some(assignee) = t.assignee_user_id {
            ids.push(assignee);
        }
    }
    let users = resolve::user_map(&state.user, &state.group, ids).await?;
    Ok(tickets
        .iter()
        .map(|t| {
            let requester = resolve::summary_from(&users, t.requester_user_id);
            let assignee = t.assignee_user_id.map(|a| resolve::summary_from(&users, a));
            dto::ticket_dto(t, requester, assignee)
        })
        .collect())
}
