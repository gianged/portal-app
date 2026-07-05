//! JWT session verification as a `route_layer` on the protected sub-router: reads
//! and verifies the session cookie, then stashes the resolved [`AuthUser`] in extensions.

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use tracing::{Span, field};
use uuid::Uuid;

use crate::{
    app::AppState,
    auth::SESSION_COOKIE,
    error::{AppError, AuthError},
    extractors::auth_user::AuthUser,
};

/// Token identity of the verified session, inserted alongside [`AuthUser`] so
/// long-lived connections (the chat WebSocket) can re-check revocation
/// mid-flight instead of trusting the upgrade-time verdict forever.
#[derive(Debug, Clone, Copy)]
pub struct SessionAuth {
    pub jti: Uuid,
    pub version: u64,
}

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
    let verified = state.token.verify(token)?;
    // Revocation check (logout denylist / stale version); fail-closed.
    if state
        .revocation
        .is_revoked(verified.jti)
        .await
        .map_err(application::Error::from)?
    {
        return Err(AuthError::Invalid.into());
    }
    if state
        .revocation
        .version(verified.user_id)
        .await
        .map_err(application::Error::from)?
        != verified.version
    {
        return Err(AuthError::Invalid.into());
    }
    // Enrich the enclosing `http` span now that the caller is known.
    Span::current().record("user_id", field::display(verified.user_id.0));
    req.extensions_mut().insert(AuthUser {
        user_id: verified.user_id,
    });
    req.extensions_mut().insert(SessionAuth {
        jti: verified.jti,
        version: verified.version,
    });
    Ok(next.run(req).await)
}
