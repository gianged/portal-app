//! Overtime endpoints. Staff file and cancel their own requests; a leader decides
//! for their group, then HR decides leader-approved requests.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing,
};
use serde::Deserialize;
use uuid::Uuid;

use domain::{
    ids::{GroupId, OvertimeId, UserId},
    model::Overtime,
};
use shared::{
    dto::overtime::{CreateOvertimeRequest, DecideOvertimeRequest, OvertimeDto},
    validation::overtime::validate_overtime,
};

use crate::{
    app::AppState, dto, error::AppError, extractors::auth_user::AuthUser, resolve,
    routes::parse_date,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/overtime", routing::post(create).get(list_mine))
        .route("/overtime/queue/leader", routing::get(leader_queue))
        .route("/overtime/queue/hr", routing::get(hr_queue))
        .route("/overtime/{id}/cancel", routing::post(cancel))
        .route(
            "/overtime/{id}/leader-decision",
            routing::post(leader_decision),
        )
        .route("/overtime/{id}/hr-decision", routing::post(hr_decision))
}

#[derive(Deserialize)]
struct RangeQuery {
    from: String,
    to: String,
}

#[derive(Deserialize)]
struct GroupQuery {
    group: Uuid,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateOvertimeRequest>,
) -> Result<Json<OvertimeDto>, AppError> {
    validate_overtime(&body).map_err(|e| AppError::Validation(e.to_string()))?;
    let work_date = parse_date(&body.work_date)?;
    let cmd = dto::create_overtime_command(work_date, body);
    let overtime = state.overtime.create(auth.user_id, cmd).await?;
    Ok(Json(single(&state, &overtime).await?))
}

async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<OvertimeDto>>, AppError> {
    let from = parse_date(&q.from)?;
    let to = parse_date(&q.to)?;
    let list = state.overtime.list_mine(auth.user_id, from, to).await?;
    Ok(Json(many(&state, list).await?))
}

async fn leader_queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<GroupQuery>,
) -> Result<Json<Vec<OvertimeDto>>, AppError> {
    let list = state
        .overtime
        .list_leader_queue(auth.user_id, GroupId(q.group))
        .await?;
    Ok(Json(many(&state, list).await?))
}

async fn hr_queue(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<OvertimeDto>>, AppError> {
    let list = state.overtime.list_hr_queue(auth.user_id).await?;
    Ok(Json(many(&state, list).await?))
}

async fn cancel(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<OvertimeDto>, AppError> {
    let overtime = state.overtime.cancel(auth.user_id, OvertimeId(id)).await?;
    Ok(Json(single(&state, &overtime).await?))
}

async fn leader_decision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DecideOvertimeRequest>,
) -> Result<Json<OvertimeDto>, AppError> {
    let cmd = dto::decide_overtime_command(body);
    let overtime = state
        .overtime
        .leader_decide(auth.user_id, OvertimeId(id), cmd)
        .await?;
    Ok(Json(single(&state, &overtime).await?))
}

async fn hr_decision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<DecideOvertimeRequest>,
) -> Result<Json<OvertimeDto>, AppError> {
    let cmd = dto::decide_overtime_command(body);
    let overtime = state
        .overtime
        .hr_decide(auth.user_id, OvertimeId(id), cmd)
        .await?;
    Ok(Json(single(&state, &overtime).await?))
}

/// Resolves one request's requester + decider summaries.
async fn single(state: &AppState, overtime: &Overtime) -> Result<OvertimeDto, AppError> {
    let requester =
        resolve::user_summary(&state.user, &state.group, overtime.requester_user_id).await?;
    let leader =
        resolve::opt_user_summary(&state.user, &state.group, overtime.leader_user_id).await?;
    let hr = resolve::opt_user_summary(&state.user, &state.group, overtime.hr_user_id).await?;
    Ok(dto::overtime_dto(overtime, requester, leader, hr))
}

/// Resolves a batch of requests, deduplicating user lookups.
async fn many(state: &AppState, list: Vec<Overtime>) -> Result<Vec<OvertimeDto>, AppError> {
    let mut ids: Vec<UserId> = Vec::with_capacity(list.len() * 3);
    for o in &list {
        ids.push(o.requester_user_id);
        if let Some(l) = o.leader_user_id {
            ids.push(l);
        }
        if let Some(h) = o.hr_user_id {
            ids.push(h);
        }
    }
    let users = resolve::user_map(&state.user, &state.group, ids).await?;
    Ok(list
        .iter()
        .map(|o| {
            let requester = resolve::summary_from(&users, o.requester_user_id);
            let leader = o.leader_user_id.map(|id| resolve::summary_from(&users, id));
            let hr = o.hr_user_id.map(|id| resolve::summary_from(&users, id));
            dto::overtime_dto(o, requester, leader, hr)
        })
        .collect())
}
