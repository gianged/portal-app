//! Leave-balance endpoints. A user reads their own balance and statement; a
//! leader or HR may read a member's; HR sets grants and posts adjustments.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing,
};
use serde::Deserialize;
use time::{Date, OffsetDateTime};

use domain::ids::UserId;
use shared::dto::{
    ids as wire,
    leave_balance::{
        AdjustBalanceRequest, LeaveBalanceDto, LeaveStatementDto, SetLeaveGrantRequest,
    },
};

use crate::{
    app::AppState,
    dto,
    error::AppError,
    extractors::{auth_user::AuthUser, validated_json::ValidatedJson},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/leave/balance", routing::get(my_balance))
        .route("/leave/statement", routing::get(my_statement))
        .route("/users/{id}/leave/balance", routing::get(user_balance))
        .route("/users/{id}/leave/grant", routing::put(set_grant))
        .route("/users/{id}/leave/adjust", routing::post(adjust))
}

#[derive(Deserialize)]
struct RangeQuery {
    from: Date,
    to: Date,
}

async fn my_balance(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<LeaveBalanceDto>, AppError> {
    Ok(Json(balance_for(&state, auth.user_id, auth.user_id).await?))
}

async fn user_balance(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::UserId>,
) -> Result<Json<LeaveBalanceDto>, AppError> {
    Ok(Json(balance_for(&state, auth.user_id, UserId(id.0)).await?))
}

async fn my_statement(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RangeQuery>,
) -> Result<Json<LeaveStatementDto>, AppError> {
    let (grants, txns) = state.leave.statement(auth.user_id, q.from, q.to).await?;
    Ok(Json(dto::leave_statement_dto(&grants, &txns)))
}

async fn set_grant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::UserId>,
    ValidatedJson(body): ValidatedJson<SetLeaveGrantRequest>,
) -> Result<Json<LeaveBalanceDto>, AppError> {
    let target = UserId(id.0);
    let cmd = dto::set_leave_grant_command(target, &body);
    state.leave.set_grant(auth.user_id, cmd).await?;
    Ok(Json(balance_for(&state, auth.user_id, target).await?))
}

async fn adjust(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::UserId>,
    ValidatedJson(body): ValidatedJson<AdjustBalanceRequest>,
) -> Result<Json<LeaveBalanceDto>, AppError> {
    let target = UserId(id.0);
    let cmd = dto::adjust_balance_command(target, body);
    state.leave.adjust(auth.user_id, cmd).await?;
    Ok(Json(balance_for(&state, auth.user_id, target).await?))
}

/// Available days (as of today) plus the per-year grant breakdown; the service
/// gates who may view whom.
async fn balance_for(
    state: &AppState,
    actor: UserId,
    target: UserId,
) -> Result<LeaveBalanceDto, AppError> {
    let today = OffsetDateTime::now_utc().date();
    let available = state.leave.balance_of(actor, target, today).await?;
    let grants = state.leave.grants_of(actor, target).await?;
    Ok(dto::leave_balance_dto(available, &grants))
}
