//! Authentication endpoints: login + logout (public) and `/me` (protected).

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use time::OffsetDateTime;

use shared::{
    dto::user::{ChangePasswordRequest, LoginRequest, LoginResponse, UserDto},
    validation::user::validate_change_password,
};

use crate::{
    app::AppState,
    auth::SESSION_COOKIE,
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

/// Authenticated identity endpoints, mounted under the protected router.
pub fn me_router() -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route("/me/password", post(change_password))
}

async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<LoginRequest>,
) -> Result<(CookieJar, Json<LoginResponse>), AppError> {
    let Some(user) = state.user.login(&body.email, &body.password).await? else {
        return Err(AuthError::InvalidCredentials.into());
    };
    // Mint at the user's current token version so the session survives until
    // the next version bump (deactivation, password change).
    let ver = state
        .revocation
        .version(user.id)
        .await
        .map_err(application::Error::from)?;
    let token = state.token.issue(user.id, ver);
    let jar = jar.add(state.token.session_cookie(token));
    let role = resolve::role_for_user(&state.group, &user).await?;
    Ok((
        jar,
        Json(LoginResponse {
            user: dto::user_dto(&user, role),
        }),
    ))
}

async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), AppError> {
    // Denylist the presented token for its remaining lifetime so clearing the
    // cookie actually ends the session server-side. An absent or invalid
    // cookie still clears cleanly; a Redis failure is a real error — don't
    // claim a logout that didn't happen.
    if let Some(token) = jar.get(SESSION_COOKIE).map(Cookie::value)
        && let Ok(verified) = state.token.verify(token)
    {
        let remaining = verified.exp - OffsetDateTime::now_utc().unix_timestamp();
        state
            .revocation
            .revoke(verified.jti, u64::try_from(remaining).unwrap_or(0))
            .await
            .map_err(application::Error::from)?;
    }
    Ok((jar.add(state.token.clear_cookie()), StatusCode::NO_CONTENT))
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

/// Self-service password change. Existing sessions (including this one) are
/// revoked; the client should drop its state and re-login.
async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<StatusCode, AppError> {
    validate_change_password(&body).map_err(|e| AppError::Validation(e.to_string()))?;
    state
        .user
        .change_password(auth.user_id, &body.current_password, body.new_password)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
