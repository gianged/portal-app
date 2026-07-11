//! Company / group announcements (immutable after a 15-minute edit grace,
//! enforced in the domain).

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing,
};
use serde::Deserialize;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    ids::{ChannelId, MessageId},
    model::Announcement,
};
use shared::dto::{
    announcement::{AnnouncementDto, EditAnnouncementRequest, PostAnnouncementRequest},
    ids as wire,
};

use crate::{
    app::AppState,
    dto,
    error::AppError,
    extractors::{auth_user::AuthUser, validated_json::ValidatedJson},
    resolve,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/announcements", routing::post(post_announcement).get(list))
        .route(
            "/announcements/{channel_id}/{announcement_id}",
            routing::patch(edit).delete(remove),
        )
}

#[derive(Deserialize)]
struct ListQuery {
    channel: Uuid,
}

async fn post_announcement(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<PostAnnouncementRequest>,
) -> Result<Json<AnnouncementDto>, AppError> {
    let announcement = state
        .announcement
        .post(auth.user_id, dto::post_announcement_command(body))
        .await?;
    Ok(Json(to_dto(&state, &announcement).await?))
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<AnnouncementDto>>, AppError> {
    let announcements = state
        .announcement
        .list_for_channel(auth.user_id, ChannelId(q.channel))
        .await?;
    let now = OffsetDateTime::now_utc();
    let ids: Vec<_> = announcements.iter().map(|a| a.sender_user_id).collect();
    let users = resolve::user_map(&state.user, &state.group, ids).await?;
    let out = announcements
        .iter()
        .map(|a| dto::announcement_dto(a, resolve::summary_from(&users, a.sender_user_id), now))
        .collect();
    Ok(Json(out))
}

async fn edit(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((channel_id, announcement_id)): Path<(wire::ChannelId, wire::MessageId)>,
    ValidatedJson(body): ValidatedJson<EditAnnouncementRequest>,
) -> Result<Json<AnnouncementDto>, AppError> {
    let announcement = state
        .announcement
        .edit(
            auth.user_id,
            ChannelId(channel_id.0),
            MessageId(announcement_id.0),
            body.body,
        )
        .await?;
    Ok(Json(to_dto(&state, &announcement).await?))
}

async fn remove(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((channel_id, announcement_id)): Path<(wire::ChannelId, wire::MessageId)>,
) -> Result<StatusCode, AppError> {
    state
        .announcement
        .delete(
            auth.user_id,
            ChannelId(channel_id.0),
            MessageId(announcement_id.0),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn to_dto(
    state: &AppState,
    announcement: &Announcement,
) -> Result<AnnouncementDto, AppError> {
    let sender =
        resolve::user_summary(&state.user, &state.group, announcement.sender_user_id).await?;
    Ok(dto::announcement_dto(
        announcement,
        sender,
        OffsetDateTime::now_utc(),
    ))
}
