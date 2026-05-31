//! In-app notification endpoints.

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use domain::ids::NotificationId;
use shared::dto::notification::{MarkReadRequest, NotificationDto};

use crate::{app::AppState, dto, error::AppError, extractors::auth_user::AuthUser};

/// Cap on how many unread notifications a single "mark all" sweep touches.
const MARK_ALL_LIMIT: u32 = 500;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(list))
        .route("/notifications/unread-count", get(unread_count))
        .route("/notifications/mark-read", post(mark_read))
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
) -> Result<Json<Value>, AppError> {
    let count = state.notification.count_unread(auth.user_id).await?;
    Ok(Json(json!({ "count": count })))
}

/// Marks the listed notifications read. An empty list means "mark all unread"
/// (bounded by `MARK_ALL_LIMIT`).
async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<MarkReadRequest>,
) -> Result<StatusCode, AppError> {
    let ids: Vec<NotificationId> = if body.notification_ids.is_empty() {
        state
            .notification
            .list_for_user(auth.user_id, true, MARK_ALL_LIMIT)
            .await?
            .iter()
            .map(|n| n.id)
            .collect()
    } else {
        body.notification_ids
            .iter()
            .map(|id| NotificationId(id.0))
            .collect()
    };
    for id in ids {
        state.notification.mark_read(auth.user_id, id).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}
