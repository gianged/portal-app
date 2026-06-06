//! In-app notification data: the unread badge count, the inbox list, and the
//! mark-read flow.

use serde::Deserialize;

use shared::dto::ids::NotificationId;
use shared::dto::notification::{MarkReadRequest, NotificationDto};

use crate::api::client;
use crate::api::error::FrontendError;

/// `GET /notifications/unread-count` returns a bare `{ "count": <n> }` object
/// (no shared DTO), so it is decoded into this local shape.
#[derive(Deserialize)]
struct UnreadCount {
    count: u64,
}

/// Number of unread notifications for the current user (topbar badge).
pub async fn unread_count() -> Result<u64, FrontendError> {
    let resp: UnreadCount = client::get_json("/notifications/unread-count").await?;
    Ok(resp.count)
}

/// The inbox list (`GET /notifications`), optionally unread-only.
pub async fn list(unread_only: bool, limit: u32) -> Result<Vec<NotificationDto>, FrontendError> {
    let limit_s = limit.to_string();
    let q = if unread_only {
        client::query(&[("unread_only", "true"), ("limit", &limit_s)])
    } else {
        client::query(&[("limit", &limit_s)])
    };
    client::get_json(&format!("/notifications{q}")).await
}

/// Mark notifications read; an empty list means "mark all" (`POST /notifications/mark-read`).
pub async fn mark_read(ids: Vec<NotificationId>) -> Result<(), FrontendError> {
    let req = MarkReadRequest {
        notification_ids: ids,
    };
    client::post_json_no_content("/notifications/mark-read", &req).await
}
