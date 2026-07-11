//! Day-off (leave) endpoints. Staff file and cancel their own requests; a leader
//! decides any kind for their group, and HR decides leader-approved annual leave.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing,
};
use serde::Deserialize;
use time::Date;
use uuid::Uuid;

use domain::{
    ids::{DayOffId, GroupId, UserId},
    model::DayOff,
};
use shared::dto::{
    day_off::{CreateDayOffRequest, DayOffDto, DecideDayOffRequest},
    ids as wire,
};

use crate::{
    app::AppState,
    dto,
    error::AppError,
    extractors::{auth_user::AuthUser, validated_json::ValidatedJson},
    resolve,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/dayoff", routing::post(create).get(list_mine))
        .route("/dayoff/queue/leader", routing::get(leader_queue))
        .route("/dayoff/queue/hr", routing::get(hr_queue))
        .route("/dayoff/{id}/cancel", routing::post(cancel))
        .route(
            "/dayoff/{id}/leader-decision",
            routing::post(leader_decision),
        )
        .route("/dayoff/{id}/hr-decision", routing::post(hr_decision))
}

#[derive(Deserialize)]
struct RangeQuery {
    from: Date,
    to: Date,
}

#[derive(Deserialize)]
struct GroupQuery {
    group: Uuid,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<CreateDayOffRequest>,
) -> Result<Json<DayOffDto>, AppError> {
    let cmd = dto::create_day_off_command(body);
    let day_off = state.day_off.create(auth.user_id, cmd).await?;
    Ok(Json(single(&state, &day_off).await?))
}

async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<DayOffDto>>, AppError> {
    let list = state.day_off.list_mine(auth.user_id, q.from, q.to).await?;
    Ok(Json(many(&state, list).await?))
}

async fn leader_queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<GroupQuery>,
) -> Result<Json<Vec<DayOffDto>>, AppError> {
    let list = state
        .day_off
        .list_leader_queue(auth.user_id, GroupId(q.group))
        .await?;
    Ok(Json(many(&state, list).await?))
}

async fn hr_queue(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<DayOffDto>>, AppError> {
    let list = state.day_off.list_hr_queue(auth.user_id).await?;
    Ok(Json(many(&state, list).await?))
}

async fn cancel(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::DayOffId>,
) -> Result<Json<DayOffDto>, AppError> {
    let day_off = state.day_off.cancel(auth.user_id, DayOffId(id.0)).await?;
    Ok(Json(single(&state, &day_off).await?))
}

async fn leader_decision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::DayOffId>,
    ValidatedJson(body): ValidatedJson<DecideDayOffRequest>,
) -> Result<Json<DayOffDto>, AppError> {
    let cmd = dto::decide_day_off_command(body);
    let day_off = state
        .day_off
        .leader_decide(auth.user_id, DayOffId(id.0), cmd)
        .await?;
    Ok(Json(single(&state, &day_off).await?))
}

async fn hr_decision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::DayOffId>,
    ValidatedJson(body): ValidatedJson<DecideDayOffRequest>,
) -> Result<Json<DayOffDto>, AppError> {
    let cmd = dto::decide_day_off_command(body);
    let day_off = state
        .day_off
        .hr_decide(auth.user_id, DayOffId(id.0), cmd)
        .await?;
    Ok(Json(single(&state, &day_off).await?))
}

/// Resolves one request's requester + decider summaries.
async fn single(state: &AppState, day_off: &DayOff) -> Result<DayOffDto, AppError> {
    let requester =
        resolve::user_summary(&state.user, &state.group, day_off.requester_user_id).await?;
    let leader =
        resolve::opt_user_summary(&state.user, &state.group, day_off.leader_user_id).await?;
    let hr = resolve::opt_user_summary(&state.user, &state.group, day_off.hr_user_id).await?;
    Ok(dto::day_off_dto(day_off, requester, leader, hr))
}

/// Resolves a batch of requests, deduplicating user lookups.
async fn many(state: &AppState, list: Vec<DayOff>) -> Result<Vec<DayOffDto>, AppError> {
    let mut ids: Vec<UserId> = Vec::with_capacity(list.len() * 3);
    for d in &list {
        ids.push(d.requester_user_id);
        if let Some(l) = d.leader_user_id {
            ids.push(l);
        }
        if let Some(h) = d.hr_user_id {
            ids.push(h);
        }
    }
    let users = resolve::user_map(&state.user, &state.group, ids).await?;
    Ok(list
        .iter()
        .map(|d| {
            let requester = resolve::summary_from(&users, d.requester_user_id);
            let leader = d.leader_user_id.map(|id| resolve::summary_from(&users, id));
            let hr = d.hr_user_id.map(|id| resolve::summary_from(&users, id));
            dto::day_off_dto(d, requester, leader, hr)
        })
        .collect())
}
