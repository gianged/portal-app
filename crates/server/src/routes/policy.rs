//! Attendance policy endpoints. Reading the limits is open to any authed user
//! (the UI needs them); editing is Director/HR, gated inside `PolicyService`.

use axum::{Json, Router, extract::State, routing};

use shared::dto::policy::{PolicyDto, UpdatePolicyRequest};

use crate::{
    app::AppState,
    dto,
    error::AppError,
    extractors::{auth_user::AuthUser, validated_json::ValidatedJson},
};

pub fn router() -> Router<AppState> {
    Router::new().route("/policy", routing::get(get_policy).put(update_policy))
}

async fn get_policy(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<PolicyDto>, AppError> {
    Ok(Json(dto::policy_dto(&state.policy.current())))
}

async fn update_policy(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<UpdatePolicyRequest>,
) -> Result<Json<PolicyDto>, AppError> {
    let cmd = dto::update_policy_command(body).map_err(|e| AppError::Validation(e.to_string()))?;
    let policy = state.policy.update(auth.user_id, cmd).await?;
    Ok(Json(dto::policy_dto(&policy)))
}
