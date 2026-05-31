//! Authentication endpoints: login + logout (public) and `/me` (protected).

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use axum_extra::extract::cookie::CookieJar;

use shared::dto::user::{LoginRequest, LoginResponse, UserDto};

use crate::{
    app::AppState,
    dto,
    error::{AppError, AuthError},
    extractors::auth_user::AuthUser,
    resolve,
};

/// Unauthenticated endpoints. Mounted outside the auth layer.
pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
}

/// Authenticated identity endpoint, mounted under the protected router.
pub fn me_router() -> Router<AppState> {
    Router::new().route("/me", get(me))
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<LoginRequest>,
) -> Result<(CookieJar, Json<LoginResponse>), AppError> {
    let Some(user) = state.user.login(&body.email, &body.password).await? else {
        return Err(AuthError::InvalidCredentials.into());
    };
    let token = state.token.issue(user.id);
    let jar = jar.add(state.token.session_cookie(token));
    let role = resolve::role_for_user(&state.group, &user).await?;
    Ok((
        jar,
        Json(LoginResponse {
            user: dto::user_dto(&user, role),
        }),
    ))
}

async fn logout(State(state): State<AppState>, jar: CookieJar) -> (CookieJar, StatusCode) {
    (jar.add(state.token.clear_cookie()), StatusCode::NO_CONTENT)
}

async fn me(State(state): State<AppState>, auth: AuthUser) -> Result<Json<UserDto>, AppError> {
    let user = state
        .user
        .find(auth.user_id)
        .await?
        .ok_or(application::Error::NotFound("user"))?;
    let role = resolve::role_for_user(&state.group, &user).await?;
    Ok(Json(dto::user_dto(&user, role)))
}
