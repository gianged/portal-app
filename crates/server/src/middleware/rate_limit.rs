//! Fixed-window rate limiting, layered over four planes:
//!
//! - [`per_ip`] guards the public auth routes by client IP with a deliberately
//!   loose ceiling (one office NAT fronts many users). It degrades to a shared
//!   `login:unknown` bucket when no [`ConnectInfo`] is attached (e.g. under
//!   `oneshot` tests).
//! - the login handler additionally enforces a tight per-(IP, email) bucket via
//!   [`enforce`], the actual brute-force gate.
//! - [`per_user`] guards the protected API by caller id, read from the
//!   [`AuthUser`] the auth layer inserts, so it must sit *inside* the auth layer.
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

use crate::{
    app::AppState, error::AppError, extractors::auth_user::AuthUser,
    middleware::ip_allowlist::ClientIp,
};

/// Per-window request ceilings consulted by the rate-limit middleware. Held in
/// `AppState`, populated from `Config`.
#[derive(Clone, Copy)]
pub struct RateLimits {
    /// Ceiling for `/login` attempts per (client IP, email) pair.
    pub auth: u64,
    /// Ceiling for the public auth routes per client IP. Loose: behind one
    /// office NAT this bucket is shared by every user.
    pub auth_ip: u64,
    /// Ceiling for protected API calls, per authenticated user.
    pub api: u64,
    /// Ceiling for WebSocket `SendMessage` frames, per authenticated user.
    /// Applied by [`within_chat_rate`] per frame, not as a middleware layer.
    pub chat: u64,
}

/// Per-IP limiter for the public auth routes. Prefers the trusted-proxy-resolved
/// [`ClientIp`] the allowlist gate inserted (behind a proxy the raw peer is the
/// proxy, which would collapse every user into one bucket), falling back to the
/// `ConnectInfo` peer; absent both (e.g. under `oneshot` tests) all callers share
/// a `login:unknown` bucket.
pub async fn per_ip(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let ip = bucket_ip(
        req.extensions().get::<ClientIp>().copied(),
        req.extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0),
    );
    enforce(&state, &format!("login:{ip}"), state.rate_limits.auth_ip).await?;
    Ok(next.run(req).await)
}

/// Limiter identity for the auth planes: trusted-proxy client IP, then the raw
/// peer, then a shared `unknown` bucket (e.g. under `oneshot` tests).
pub fn bucket_ip(client_ip: Option<ClientIp>, peer: Option<SocketAddr>) -> String {
    client_ip.map_or_else(
        || peer.map_or_else(|| "unknown".to_owned(), |p| p.ip().to_string()),
        |c| c.0.to_string(),
    )
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
pub async fn enforce(state: &AppState, bucket: &str, limit: u64) -> Result<(), AppError> {
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
