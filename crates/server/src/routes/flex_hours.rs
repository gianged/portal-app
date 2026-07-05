//! Flexible-hours endpoints. Staff file and cancel their own per-day schedules; a
//! leader decides for their group. The monthly settlement delta is read-only.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing,
};
use serde::Deserialize;
use uuid::Uuid;

use domain::{
    ids::{FlexHoursId, GroupId, UserId},
    model::FlexHours,
};
use shared::{
    dto::flex_hours::{DecideFlexRequest, FlexHoursDto, FlexMonthDeltaDto, RequestFlexRequest},
    validation::flex_hours::validate_flex,
};

use crate::{
    app::AppState,
    dto,
    error::AppError,
    extractors::{auth_user::AuthUser, validated_json::ValidatedJson},
    resolve, routes,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/flex-hours", routing::post(create).get(list_mine))
        .route("/flex-hours/month-delta", routing::get(month_delta))
        .route("/flex-hours/queue/leader", routing::get(leader_queue))
        .route("/flex-hours/{id}/cancel", routing::post(cancel))
        .route("/flex-hours/{id}/decision", routing::post(decision))
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

#[derive(Deserialize)]
struct MonthQuery {
    year: i32,
    // u8 so serde rejects out-of-range values instead of a truncating cast.
    month: u8,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RequestFlexRequest>,
) -> Result<Json<FlexHoursDto>, AppError> {
    let policy = dto::policy_dto(&state.policy.current());
    validate_flex(&body, &policy).map_err(|e| AppError::Validation(e.to_string()))?;
    let work_date = routes::parse_date(&body.work_date)?;
    let cmd = dto::request_flex_command(work_date, body)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let flex = state.flex.request(auth.user_id, cmd).await?;
    Ok(Json(single(&state, &flex).await?))
}

async fn list_mine(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<FlexHoursDto>>, AppError> {
    let from = routes::parse_date(&q.from)?;
    let to = routes::parse_date(&q.to)?;
    let list = state.flex.list_mine(auth.user_id, from, to).await?;
    Ok(Json(many(&state, list).await?))
}

async fn month_delta(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<MonthQuery>,
) -> Result<Json<FlexMonthDeltaDto>, AppError> {
    let delta = state
        .flex
        .month_delta(auth.user_id, q.year, u32::from(q.month))
        .await?;
    Ok(Json(FlexMonthDeltaDto {
        year: q.year,
        month: q.month,
        delta,
    }))
}

async fn leader_queue(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<GroupQuery>,
) -> Result<Json<Vec<FlexHoursDto>>, AppError> {
    let list = state
        .flex
        .list_leader_queue(auth.user_id, GroupId(q.group))
        .await?;
    Ok(Json(many(&state, list).await?))
}

async fn cancel(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<FlexHoursDto>, AppError> {
    let flex = state.flex.cancel(auth.user_id, FlexHoursId(id)).await?;
    Ok(Json(single(&state, &flex).await?))
}

async fn decision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(body): ValidatedJson<DecideFlexRequest>,
) -> Result<Json<FlexHoursDto>, AppError> {
    let cmd = dto::decide_flex_command(body);
    let flex = state
        .flex
        .decide(auth.user_id, FlexHoursId(id), cmd)
        .await?;
    Ok(Json(single(&state, &flex).await?))
}

/// Resolves one request's owner + leader summaries.
async fn single(state: &AppState, flex: &FlexHours) -> Result<FlexHoursDto, AppError> {
    let user = resolve::user_summary(&state.user, &state.group, flex.user_id).await?;
    let leader = resolve::opt_user_summary(&state.user, &state.group, flex.leader_user_id).await?;
    Ok(dto::flex_hours_dto(flex, user, leader))
}

/// Resolves a batch of requests, deduplicating user lookups.
async fn many(state: &AppState, list: Vec<FlexHours>) -> Result<Vec<FlexHoursDto>, AppError> {
    let mut ids: Vec<UserId> = Vec::with_capacity(list.len() * 2);
    for f in &list {
        ids.push(f.user_id);
        if let Some(l) = f.leader_user_id {
            ids.push(l);
        }
    }
    let users = resolve::user_map(&state.user, &state.group, ids).await?;
    Ok(list
        .iter()
        .map(|f| {
            let user = resolve::summary_from(&users, f.user_id);
            let leader = f.leader_user_id.map(|id| resolve::summary_from(&users, id));
            dto::flex_hours_dto(f, user, leader)
        })
        .collect())
}
