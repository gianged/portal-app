//! IT ticket endpoints. Listing is scope-filtered via a query param to avoid a
//! static/param routing conflict with `/tickets/{id}`.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing,
};
use serde::Deserialize;

use domain::{
    ids::{CommentId, TicketId, UserId},
    model::{CommentEntity, Ticket},
};
use shared::dto::{
    comment::{CommentDto, CreateCommentRequest, UpdateCommentRequest},
    ids as wire,
    ticket::{AssignTicketRequest, RaiseTicketRequest, TicketDto, TriageTicketRequest},
};

use crate::{
    app::AppState,
    dto,
    error::AppError,
    extractors::{app_json::AppJson, auth_user::AuthUser, validated_json::ValidatedJson},
    resolve,
    routes::{
        self,
        comments::{self, CommentsQuery},
    },
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tickets", routing::post(raise).get(list))
        .route("/tickets/{id}", routing::get(get_one))
        .route("/tickets/{id}/triage", routing::post(triage))
        .route("/tickets/{id}/assign", routing::post(assign))
        .route("/tickets/{id}/start", routing::post(start))
        .route("/tickets/{id}/resolve", routing::post(resolve_ticket))
        .route("/tickets/{id}/reject", routing::post(reject))
        .route("/tickets/{id}/close", routing::post(close))
        .route("/tickets/{id}/reopen", routing::post(reopen))
        .route(
            "/tickets/{id}/comments",
            routing::get(list_comments).post(add_comment),
        )
        .route(
            "/tickets/{id}/comments/{comment_id}",
            routing::patch(edit_comment).delete(delete_comment),
        )
}

fn entity(id: wire::TicketId) -> CommentEntity {
    CommentEntity::Ticket {
        ticket_id: TicketId(id.0),
    }
}

async fn list_comments(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::TicketId>,
    Query(q): Query<CommentsQuery>,
) -> Result<Json<Vec<CommentDto>>, AppError> {
    Ok(Json(
        comments::list(&state, auth.user_id, entity(id), &q).await?,
    ))
}

async fn add_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::TicketId>,
    ValidatedJson(body): ValidatedJson<CreateCommentRequest>,
) -> Result<Json<CommentDto>, AppError> {
    Ok(Json(
        comments::add(&state, auth.user_id, entity(id), body.body).await?,
    ))
}

async fn edit_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, comment_id)): Path<(wire::TicketId, wire::CommentId)>,
    ValidatedJson(body): ValidatedJson<UpdateCommentRequest>,
) -> Result<Json<CommentDto>, AppError> {
    Ok(Json(
        comments::edit(
            &state,
            auth.user_id,
            entity(id),
            CommentId(comment_id.0),
            body.body,
        )
        .await?,
    ))
}

async fn delete_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, comment_id)): Path<(wire::TicketId, wire::CommentId)>,
) -> Result<StatusCode, AppError> {
    comments::remove(&state, auth.user_id, entity(id), CommentId(comment_id.0)).await?;
    Ok(StatusCode::NO_CONTENT)
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
    /// Substring search on the ticket title.
    q: Option<String>,
}

async fn raise(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<RaiseTicketRequest>,
) -> Result<Json<TicketDto>, AppError> {
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
    let search = routes::norm_q(q.q);
    let search = search.as_deref();
    let tickets = match q.scope.unwrap_or(Scope::Mine) {
        Scope::Triage => {
            let limit = q.limit.unwrap_or(50).clamp(1, 200);
            state
                .ticket
                .list_open_for_triage(auth.user_id, limit, search)
                .await?
        }
        Scope::Assigned => state.ticket.list_for_assignee(auth.user_id, search).await?,
        Scope::Mine => {
            state
                .ticket
                .list_for_requester(auth.user_id, search)
                .await?
        }
    };
    Ok(Json(many(&state, tickets).await?))
}

async fn get_one(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::TicketId>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state.ticket.find(auth.user_id, TicketId(id.0)).await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn triage(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::TicketId>,
    AppJson(body): AppJson<TriageTicketRequest>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state
        .ticket
        .triage(
            auth.user_id,
            TicketId(id.0),
            dto::ticket_priority_domain(body.priority),
        )
        .await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn assign(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::TicketId>,
    AppJson(body): AppJson<AssignTicketRequest>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state
        .ticket
        .assign(
            auth.user_id,
            TicketId(id.0),
            UserId(body.assignee_user_id.0),
        )
        .await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn start(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::TicketId>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state.ticket.start(auth.user_id, TicketId(id.0)).await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn resolve_ticket(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::TicketId>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state.ticket.resolve(auth.user_id, TicketId(id.0)).await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn reject(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::TicketId>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state
        .ticket
        .reject_resolution(auth.user_id, TicketId(id.0))
        .await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn close(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::TicketId>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state.ticket.close(auth.user_id, TicketId(id.0)).await?;
    Ok(Json(single(&state, &ticket).await?))
}

async fn reopen(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::TicketId>,
) -> Result<Json<TicketDto>, AppError> {
    let ticket = state.ticket.reopen(auth.user_id, TicketId(id.0)).await?;
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
