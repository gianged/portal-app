//! User administration (HR-gated mutations) and the user directory.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing,
};
use serde::Deserialize;

use domain::ids::UserId;
use shared::dto::{
    ids as wire,
    user::{
        CreateUserRequest, ResetPasswordRequest, UpdateProfileRequest, UserDto, UserProfileDto,
        UserRole,
    },
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
        .route("/users", routing::post(create).get(list))
        .route("/users/{id}", routing::get(get_one).patch(update))
        .route("/users/{id}/deactivate", routing::post(deactivate))
        .route("/users/{id}/reactivate", routing::post(reactivate))
        .route("/users/{id}/reset-password", routing::post(reset_password))
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
    ValidatedJson(body): ValidatedJson<CreateUserRequest>,
) -> Result<Json<UserProfileDto>, AppError> {
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
    let search = routes::norm_q(q.q);
    let users = state
        .user
        .list_active(limit, offset, search.as_deref())
        .await?;
    let mut identities = resolve::identity_map(&state.group, &users).await?;
    let out = users
        .iter()
        .map(|u| {
            let (role, memberships) = identities
                .remove(&u.id)
                .unwrap_or((UserRole::Member, Vec::new()));
            dto::user_dto(u, role, memberships)
        })
        .collect();
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(id): Path<wire::UserId>,
) -> Result<Json<UserProfileDto>, AppError> {
    let user = state
        .user
        .find(UserId(id.0))
        .await?
        .ok_or(application::Error::NotFound("user"))?;
    Ok(Json(dto::user_profile_dto(&user)))
}

async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::UserId>,
    ValidatedJson(body): ValidatedJson<UpdateProfileRequest>,
) -> Result<Json<UserProfileDto>, AppError> {
    let user = state
        .user
        .update_profile(
            auth.user_id,
            UserId(id.0),
            dto::update_profile_command(body),
        )
        .await?;
    Ok(Json(dto::user_profile_dto(&user)))
}

async fn deactivate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::UserId>,
) -> Result<StatusCode, AppError> {
    state
        .user
        .deactivate_user(auth.user_id, UserId(id.0))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reactivate(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::UserId>,
) -> Result<Json<UserProfileDto>, AppError> {
    let user = state
        .user
        .reactivate_user(auth.user_id, UserId(id.0))
        .await?;
    Ok(Json(dto::user_profile_dto(&user)))
}

/// HR sets a temporary password for the target user; their sessions die.
async fn reset_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::UserId>,
    ValidatedJson(body): ValidatedJson<ResetPasswordRequest>,
) -> Result<StatusCode, AppError> {
    state
        .user
        .admin_reset_password(auth.user_id, UserId(id.0), body.new_password)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
