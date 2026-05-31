//! JWT session verification, applied as a `route_layer` on the protected
//! sub-router. Reads the session cookie, verifies it, and stashes the resolved
//! [`AuthUser`] in the request extensions for the extractor to read. Unmatched
//! paths still 404 (no auth) because this is a `route_layer`, not a global one.

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};

use crate::{
    app::AppState,
    auth::SESSION_COOKIE,
    error::{AppError, AuthError},
    extractors::auth_user::AuthUser,
};

pub async fn require_auth(
    State(state): State<AppState>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = jar
        .get(SESSION_COOKIE)
        .map(Cookie::value)
        .ok_or(AppError::Auth(AuthError::Missing))?;
    let user_id = state.token.verify(token)?;
    req.extensions_mut().insert(AuthUser { user_id });
    Ok(next.run(req).await)
}
