//! Bearer-key authentication for the external read API (`/api/ext/v1`): resolves
//! `Authorization: Bearer pak_*` to an active service account, applies the
//! per-IP then per-key rate limits, and stashes the [`ServiceAccountCtx`] in
//! extensions.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Request, State},
    http::header,
    middleware::Next,
    response::Response,
};
use tracing::{Span, field};

use domain::ids::ServiceAccountId;

use crate::{
    app::AppState,
    error::{AppError, AuthError},
    middleware::{ip_allowlist::ClientIp, rate_limit},
};

/// Identity of the authenticated service account, read by the ext handlers.
#[derive(Debug, Clone, Copy)]
pub struct ServiceAccountCtx {
    pub id: ServiceAccountId,
}

pub async fn require_service_account(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // Per-IP gate ahead of authentication: an unauthenticated flood must not
    // reach the key hash + lookup.
    let ip = rate_limit::bucket_ip(
        req.extensions().get::<ClientIp>().copied(),
        req.extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0),
    );
    rate_limit::enforce(&state, &format!("ext_ip:{ip}"), state.rate_limits.ext_ip).await?;
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Auth(AuthError::Missing))?;
    let account = state
        .service_accounts
        .authenticate(presented)
        .await?
        .ok_or(AppError::Auth(AuthError::Invalid))?;
    rate_limit::enforce(
        &state,
        &format!("ext:{}", account.id.0),
        state.rate_limits.ext,
    )
    .await?;
    Span::current().record(
        "user_id",
        field::display(format_args!("sa:{}", account.id.0)),
    );
    req.extensions_mut()
        .insert(ServiceAccountCtx { id: account.id });
    Ok(next.run(req).await)
}
