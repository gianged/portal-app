//! IT ticket endpoints. Listing is scope-filtered via a query param to avoid a
//! static/param routing conflict with `/tickets/{id}`.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
};
use serde::Deserialize;
use uuid::Uuid;

use domain::{
    ids::{CommentId, TicketId, UserId},
    model::{CommentEntity, Ticket},
};
use shared::dto::comment::{CommentDto, CreateCommentRequest, UpdateCommentRequest};
use shared::dto::ticket::{
    AssignTicketRequest, RaiseTicketRequest, TicketDto, TriageTicketRequest,
};
use shared::validation::comment::validate_comment_body;
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
        .route(
            "/tickets/{id}/comments",
            get(list_comments).post(add_comment),
        )
        .route(
            "/tickets/{id}/comments/{comment_id}",
            patch(edit_comment).delete(delete_comment),
        )
}

#[derive(Deserialize)]
struct CommentsQuery {
    /// Exclusive newest-first cursor (a comment id).
    before: Option<Uuid>,
    limit: Option<u32>,
}

async fn list_comments(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<CommentsQuery>,
) -> Result<Json<Vec<CommentDto>>, AppError> {
    let entity = CommentEntity::Ticket {
        ticket_id: TicketId(id),
    };
    let before = q.before.map(CommentId);
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let comments = state
        .comment
        .list(auth.user_id, entity, before, limit)
        .await?;
    comments_to_dtos(&state, auth.user_id, comments).await
}

async fn add_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateCommentRequest>,
) -> Result<Json<CommentDto>, AppError> {
    validate_comment_body(&body.body).map_err(|e| AppError::Validation(e.to_string()))?;
    let entity = CommentEntity::Ticket {
        ticket_id: TicketId(id),
    };
    let comment = state.comment.add(auth.user_id, entity, body.body).await?;
    comment_to_dto(&state, auth.user_id, &comment).await
}

async fn edit_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, comment_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateCommentRequest>,
) -> Result<Json<CommentDto>, AppError> {
    validate_comment_body(&body.body).map_err(|e| AppError::Validation(e.to_string()))?;
    let entity = CommentEntity::Ticket {
        ticket_id: TicketId(id),
    };
    let comment = state
        .comment
        .edit(auth.user_id, entity, CommentId(comment_id), body.body)
        .await?;
    comment_to_dto(&state, auth.user_id, &comment).await
}

async fn delete_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let entity = CommentEntity::Ticket {
        ticket_id: TicketId(id),
    };
    state
        .comment
        .remove(auth.user_id, entity, CommentId(comment_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Resolves one comment's author summary.
async fn comment_to_dto(
    state: &AppState,
    viewer: UserId,
    comment: &domain::model::Comment,
) -> Result<Json<CommentDto>, AppError> {
    let author = resolve::user_summary(&state.user, &state.group, comment.author_user_id).await?;
    let now = time::OffsetDateTime::now_utc();
    Ok(Json(dto::comment_dto(comment, author, viewer, now)))
}

/// Resolves a page of comments with one deduped author lookup.
async fn comments_to_dtos(
    state: &AppState,
    viewer: UserId,
    comments: Vec<domain::model::Comment>,
) -> Result<Json<Vec<CommentDto>>, AppError> {
    let authors = resolve::user_map(
        &state.user,
        &state.group,
        comments.iter().map(|c| c.author_user_id),
    )
    .await?;
    let now = time::OffsetDateTime::now_utc();
    Ok(Json(
        comments
            .iter()
            .map(|c| {
                let author = resolve::summary_from(&authors, c.author_user_id);
                dto::comment_dto(c, author, viewer, now)
            })
            .collect(),
    ))
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
    let search = crate::routes::norm_q(q.q);
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
