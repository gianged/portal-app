//! In-app notification endpoints.

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing,
};
use serde::Deserialize;

use domain::ids::NotificationId;
use shared::dto::notification::{MarkReadRequest, NotificationDto, UnreadCountDto};

use crate::{
    app::AppState,
    dto,
    error::AppError,
    extractors::{auth_user::AuthUser, validated_json::ValidatedJson},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/notifications", routing::get(list))
        .route("/notifications/unread-count", routing::get(unread_count))
        .route("/notifications/mark-read", routing::post(mark_read))
}

#[derive(Deserialize)]
struct ListQuery {
    #[serde(default)]
    unread_only: bool,
    limit: Option<u32>,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<NotificationDto>>, AppError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let items = state
        .notification
        .list_for_user(auth.user_id, q.unread_only, limit)
        .await?;
    Ok(Json(items.iter().map(dto::notification_dto).collect()))
}

async fn unread_count(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<UnreadCountDto>, AppError> {
    let count = state.notification.count_unread(auth.user_id).await?;
    Ok(Json(UnreadCountDto { count }))
}

/// Marks the listed notifications read. An empty list means "mark all unread".
async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<MarkReadRequest>,
) -> Result<StatusCode, AppError> {
    let ids: Vec<NotificationId> = body
        .notification_ids
        .iter()
        .map(|id| NotificationId(id.0))
        .collect();
    state
        .notification
        .mark_read_many(auth.user_id, &ids)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
