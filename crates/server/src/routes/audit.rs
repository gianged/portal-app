//! Admin audit-log read endpoints. Every route is admin-gated (Director/HR) by
//! `AuditService`; non-admins get 403. Actors are resolved to summaries here,
//! with the dangling-reference fallback for hard-deleted users.

use axum::{
    Json, Router,
    extract::{Query, State},
    routing,
};
use serde::Deserialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use domain::model::AuditLog;
use shared::dto::audit::AuditLogDto;

use crate::{app::AppState, dto, error::AppError, extractors::auth_user::AuthUser, resolve};

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/audit/feed", routing::get(feed))
        .route("/audit", routing::get(for_entity))
}

#[derive(Deserialize)]
struct FeedQuery {
    limit: Option<u32>,
    /// RFC 3339 cursor; only rows strictly older than this are returned.
    before: Option<String>,
}

async fn feed(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<FeedQuery>,
) -> Result<Json<Vec<AuditLogDto>>, AppError> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let before = match q.before {
        Some(s) => Some(
            OffsetDateTime::parse(&s, &Rfc3339)
                .map_err(|_| AppError::Validation("invalid `before` timestamp".into()))?,
        ),
        None => None,
    };
    let logs = state
        .audit_service
        .list_recent(auth.user_id, limit, before)
        .await?;
    Ok(Json(project(&state, &logs).await?))
}

#[derive(Deserialize)]
struct EntityQuery {
    entity_schema: String,
    entity_table: String,
    entity_id: Uuid,
    limit: Option<u32>,
}

async fn for_entity(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<EntityQuery>,
) -> Result<Json<Vec<AuditLogDto>>, AppError> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let logs = state
        .audit_service
        .list_for_entity(
            auth.user_id,
            &q.entity_schema,
            &q.entity_table,
            q.entity_id,
            limit,
        )
        .await?;
    Ok(Json(project(&state, &logs).await?))
}

/// Resolves actor summaries for a batch of rows (one deduped fetch) and maps the
/// rows to wire DTOs.
async fn project(state: &AppState, logs: &[AuditLog]) -> Result<Vec<AuditLogDto>, AppError> {
    let actors = resolve::user_map(
        &state.user,
        &state.group,
        logs.iter().filter_map(|l| l.actor_user_id),
    )
    .await?;
    Ok(logs
        .iter()
        .map(|l| {
            let actor = l.actor_user_id.map(|id| resolve::summary_from(&actors, id));
            dto::audit_log_dto(l, actor)
        })
        .collect())
}
