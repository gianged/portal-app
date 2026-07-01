//! Leave-balance endpoints. A user reads their own balance and statement; a
//! leader or HR may read a member's; HR sets grants and posts adjustments.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing,
};
use serde::Deserialize;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::ids::UserId;
use shared::{
    dto::leave_balance::{
        AdjustBalanceRequest, LeaveBalanceDto, LeaveStatementDto, SetLeaveGrantRequest,
    },
    validation::leave_balance::{validate_adjust, validate_grant},
};

use crate::{app::AppState, dto, error::AppError, extractors::auth_user::AuthUser, routes};

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
    from: String,
    to: String,
}

async fn my_balance(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<LeaveBalanceDto>, AppError> {
    Ok(Json(balance_for(&state, auth.user_id).await?))
}

async fn user_balance(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<LeaveBalanceDto>, AppError> {
    let target = UserId(id);
    require_can_view(&state, auth.user_id, target).await?;
    Ok(Json(balance_for(&state, target).await?))
}

async fn my_statement(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<RangeQuery>,
) -> Result<Json<LeaveStatementDto>, AppError> {
    let from = routes::parse_date(&q.from)?;
    let to = routes::parse_date(&q.to)?;
    let (grants, txns) = state.leave.statement(auth.user_id, from, to).await?;
    Ok(Json(dto::leave_statement_dto(&grants, &txns)))
}

async fn set_grant(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SetLeaveGrantRequest>,
) -> Result<Json<LeaveBalanceDto>, AppError> {
    validate_grant(&body).map_err(|e| AppError::Validation(e.to_string()))?;
    let target = UserId(id);
    let cmd = dto::set_leave_grant_command(target, body);
    state.leave.set_grant(auth.user_id, cmd).await?;
    Ok(Json(balance_for(&state, target).await?))
}

async fn adjust(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AdjustBalanceRequest>,
) -> Result<Json<LeaveBalanceDto>, AppError> {
    validate_adjust(&body).map_err(|e| AppError::Validation(e.to_string()))?;
    let target = UserId(id);
    let cmd = dto::adjust_balance_command(target, body);
    state.leave.adjust(auth.user_id, cmd).await?;
    Ok(Json(balance_for(&state, target).await?))
}

/// Self always; otherwise a leader of the member or HR.
async fn require_can_view(state: &AppState, actor: UserId, target: UserId) -> Result<(), AppError> {
    if actor == target {
        return Ok(());
    }
    let allowed =
        state.perms.is_leader_of_member(actor, target).await? || state.perms.is_hr(actor).await?;
    if allowed {
        Ok(())
    } else {
        Err(application::Error::Forbidden.into())
    }
}

/// Available days (as of today) plus the per-year grant breakdown.
async fn balance_for(state: &AppState, user: UserId) -> Result<LeaveBalanceDto, AppError> {
    let today = OffsetDateTime::now_utc().date();
    let available = state.leave.available(user, today).await?;
    let grants = state.leave.grants(user).await?;
    Ok(dto::leave_balance_dto(available, &grants))
}
