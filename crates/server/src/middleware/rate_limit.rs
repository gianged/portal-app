//! Fixed-window rate limiting, layered over three planes:
//!
//! - [`per_ip`] guards the unauthenticated `/login` route by client IP, blunting
//!   credential-stuffing. It degrades to a shared `login:unknown` bucket when no
//!   [`ConnectInfo`] is attached (e.g. under `oneshot` tests).
//! - [`per_user`] guards the protected API by caller id, read from the
//!   [`AuthUser`] the auth layer inserts — so it must sit *inside* the auth layer.
//! - [`within_chat_rate`] guards the WebSocket `SendMessage` path by caller id.
//!   Unlike the other two it is NOT middleware: WS frames are not HTTP requests,
//!   so the per-connection task calls it per frame instead.
//!
//! All three call [`RateLimit::incr`] and compare the returned count against the
//! relevant ceiling in [`RateLimits`]. The HTTP planes return
//! [`AppError::RateLimited`] (429) when exceeded; the chat plane returns a bool so
//! the WS layer can answer with an error frame instead.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::Response,
};
use domain::ids::UserId;

use crate::{app::AppState, error::AppError, extractors::auth_user::AuthUser};

/// Per-window request ceilings consulted by the rate-limit middleware. Held in
/// `AppState`, populated from `Config`.
#[derive(Clone, Copy)]
pub struct RateLimits {
    /// Ceiling for unauthenticated `/login` attempts, per client IP.
    pub auth: u64,
    /// Ceiling for protected API calls, per authenticated user.
    pub api: u64,
    /// Ceiling for WebSocket `SendMessage` frames, per authenticated user.
    /// Applied by [`within_chat_rate`] per frame, not as a middleware layer.
    pub chat: u64,
}

/// Per-IP limiter for the public auth routes. Reads the peer address from the
/// `ConnectInfo` extension that `into_make_service_with_connect_info` attaches;
/// absent it (e.g. under `oneshot` tests) all callers share a `login:unknown`
/// bucket.
pub async fn per_ip(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map_or_else(|| "unknown".to_owned(), |ci| ci.0.ip().to_string());
    enforce(&state, &format!("login:{ip}"), state.rate_limits.auth).await?;
    Ok(next.run(req).await)
}

/// Per-user limiter for the protected API. Runs after the auth layer, so the
/// [`AuthUser`] extension is present; absent it (a misordered layer), the request
/// is let through rather than guessing a bucket.
pub async fn per_user(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if let Some(auth) = req.extensions().get::<AuthUser>().copied() {
        enforce(
            &state,
            &format!("api:{}", auth.user_id.0),
            state.rate_limits.api,
        )
        .await?;
    }
    Ok(next.run(req).await)
}

/// Per-user gate for WebSocket `SendMessage`, under a dedicated `chat:` bucket.
/// Returns `true` when the message may proceed. Fails open on a limiter backend
/// error: a transient blip should not drop a real message, and the ingest buffer
/// still backstops with its own overload shedding.
pub async fn within_chat_rate(state: &AppState, uid: UserId) -> bool {
    match state.rate_limiter.incr(&format!("chat:{}", uid.0)).await {
        Ok(count) => count <= state.rate_limits.chat,
        Err(e) => {
            tracing::warn!(error = %e, "ws: chat rate-limit check failed, allowing");
            true
        }
    }
}

/// Increments `bucket` and rejects with 429 once the count passes `limit`. A
/// backend failure surfaces as the wrapped repository error (500), never a silent
/// bypass.
async fn enforce(state: &AppState, bucket: &str, limit: u64) -> Result<(), AppError> {
    let count = state
        .rate_limiter
        .incr(bucket)
        .await
        .map_err(application::Error::from)?;
    if count > limit {
        return Err(AppError::RateLimited);
    }
    Ok(())
}
