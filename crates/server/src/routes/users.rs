//! User administration (HR-gated mutations) and the user directory.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use uuid::Uuid;

use domain::ids::UserId;
use shared::dto::user::{
    CreateUserRequest, ResetPasswordRequest, UpdateProfileRequest, UserDto, UserProfileDto,
    UserRole,
};
use shared::validation::user::{
    validate_create_user, validate_reset_password, validate_update_profile,
};

use crate::{app::AppState, dto, error::AppError, extractors::auth_user::AuthUser, resolve};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users", post(create).get(list))
        .route("/users/{id}", get(get_one).patch(update))
        .route("/users/{id}/deactivate", post(deactivate))
        .route("/users/{id}/reactivate", post(reactivate))
        .route("/users/{id}/reset-password", post(reset_password))
}

#[derive(Deserialize)]
struct ListQuery {
    limit: Option<u32>,
    offset: Option<u32>,
    /// Substring search on name/email.
    q: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateUserRequest>,
) -> Result<Json<UserProfileDto>, AppError> {
    validate_create_user(&body).map_err(|e| AppError::Validation(e.to_string()))?;
    let user = state
        .user
        .create_user(auth.user_id, dto::create_user_command(body))
        .await?;
    Ok(Json(dto::user_profile_dto(&user)))
}

async fn list(
    State(state): State<AppState>,
    _auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<UserDto>>, AppError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0);
    let search = crate::routes::norm_q(q.q);
    let users = state
        .user
        .list_active(limit, offset, search.as_deref())
        .await?;
    let roles = resolve::role_map(&state.group, &users).await?;
    let out = users
        .iter()
        .map(|u| dto::user_dto(u, roles.get(&u.id).copied().unwrap_or(UserRole::Member)))
        .collect();
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<UserProfileDto>, AppError> {
    let user = state
        .user
        .find(UserId(id))
        .await?
        .ok_or(application::Error::NotFound("user"))?;
    Ok(Json(dto::user_profile_dto(&user)))
}

async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<UserProfileDto>, AppError> {
    validate_update_profile(&body).map_err(|e| AppError::Validation(e.to_string()))?;
    let user = state
        .user
        .update_profile(auth.user_id, UserId(id), dto::update_profile_command(body))
        .await?;
    Ok(Json(dto::user_profile_dto(&user)))
}

async fn deactivate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state.user.deactivate_user(auth.user_id, UserId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reactivate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<UserProfileDto>, AppError> {
    let user = state.user.reactivate_user(auth.user_id, UserId(id)).await?;
    Ok(Json(dto::user_profile_dto(&user)))
}

/// HR sets a temporary password for the target user; their sessions die.
async fn reset_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ResetPasswordRequest>,
) -> Result<StatusCode, AppError> {
    validate_reset_password(&body).map_err(|e| AppError::Validation(e.to_string()))?;
    state
        .user
        .admin_reset_password(auth.user_id, UserId(id), body.new_password)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
