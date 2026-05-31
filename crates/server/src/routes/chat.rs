//! Chat REST surface: channel list, message history, posting, editing, deletion,
//! and read markers. Live delivery happens over the WebSocket (`chat_ws`); these
//! endpoints cover history and one-shot actions.
//!
//! Regular messages can be edited by their sender within the post grace window
//! (see `ChatService::edit_message`); announcements are edited via the
//! announcements routes.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use serde::Deserialize;
use uuid::Uuid;

use domain::{
    ids::{ChannelId, MessageId, UserId},
    model::{Channel, ChannelMembership, Message},
};
use shared::dto::chat::{
    ChannelDto, ChannelSummaryDto, EditMessageRequest, MessageDto, SendMessageRequest,
};
use shared::validation::chat::validate_message_body;

use crate::{app::AppState, dto, error::AppError, extractors::auth_user::AuthUser, resolve};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/chat/direct", post(open_direct))
        .route("/chat/channels", get(list_channels))
        .route(
            "/chat/channels/{id}/messages",
            get(list_messages).post(post_message),
        )
        .route(
            "/chat/channels/{id}/messages/{message_id}",
            delete(delete_message).patch(edit_message),
        )
        .route("/chat/channels/{id}/read", post(mark_read))
}

#[derive(Deserialize)]
struct OpenDirectRequest {
    user_id: Uuid,
}

#[derive(Deserialize)]
struct HistoryQuery {
    before: Option<Uuid>,
    limit: Option<u32>,
}

async fn open_direct(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<OpenDirectRequest>,
) -> Result<Json<ChannelDto>, AppError> {
    let other = UserId(body.user_id);
    let channel = state.chat.open_direct_channel(auth.user_id, other).await?;
    let other_summary = resolve::user_summary(&state.user, &state.group, other).await?;
    Ok(Json(dto::channel_dto(&channel, Some(other_summary))))
}

async fn list_channels(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<ChannelSummaryDto>>, AppError> {
    let overviews = state.chat.channel_overviews(auth.user_id).await?;
    let mut out = Vec::with_capacity(overviews.len());
    for overview in &overviews {
        let title = channel_title(&state, auth.user_id, &overview.membership).await?;
        out.push(dto::channel_summary_dto(
            &overview.membership,
            title,
            overview.unread,
            overview.last_message_at,
        ));
    }
    Ok(Json(out))
}

async fn list_messages(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<MessageDto>>, AppError> {
    let before = q.before.map(MessageId);
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let messages = state
        .chat
        .list_messages(auth.user_id, ChannelId(id), before, limit)
        .await?;
    Ok(Json(messages_to_dtos(&state, messages).await?))
}

async fn post_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SendMessageRequest>,
) -> Result<Json<MessageDto>, AppError> {
    validate_message_body(&body.body).map_err(|e| AppError::Validation(e.to_string()))?;
    let message = state
        .chat
        .post_message(auth.user_id, dto::post_message_command(ChannelId(id), body))
        .await?;
    Ok(Json(message_to_dto(&state, &message).await?))
}

async fn edit_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, message_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<EditMessageRequest>,
) -> Result<Json<MessageDto>, AppError> {
    validate_message_body(&body.body).map_err(|e| AppError::Validation(e.to_string()))?;
    let message = state
        .chat
        .edit_message(
            auth.user_id,
            ChannelId(id),
            MessageId(message_id),
            body.body,
        )
        .await?;
    Ok(Json(message_to_dto(&state, &message).await?))
}

async fn delete_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, message_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    state
        .chat
        .delete_message(auth.user_id, ChannelId(id), MessageId(message_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    state.chat.mark_read(auth.user_id, ChannelId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Sidebar title: group name, "General", or the other DM participant's name.
async fn channel_title(
    state: &AppState,
    viewer: UserId,
    membership: &ChannelMembership,
) -> Result<String, AppError> {
    let Some(channel) = state.chat.find_channel(membership.channel_id).await? else {
        return Ok("Channel".to_owned());
    };
    Ok(match channel {
        Channel::Group(c) => c.name,
        Channel::General(_) => "General".to_owned(),
        Channel::Direct(c) => {
            let other = if c.user_low_id == viewer {
                c.user_high_id
            } else {
                c.user_low_id
            };
            resolve::user_summary(&state.user, &state.group, other)
                .await?
                .full_name
        }
    })
}

/// Resolves one message's sender + mention summaries.
async fn message_to_dto(state: &AppState, message: &Message) -> Result<MessageDto, AppError> {
    let sender = resolve::user_summary(&state.user, &state.group, message.sender_user_id).await?;
    let mut mentions = Vec::with_capacity(message.mentions.len());
    for uid in &message.mentions {
        mentions.push(resolve::user_summary(&state.user, &state.group, *uid).await?);
    }
    Ok(dto::message_dto(message, sender, mentions))
}

/// Resolves a page of messages, deduplicating sender + mention lookups.
async fn messages_to_dtos(
    state: &AppState,
    messages: Vec<Message>,
) -> Result<Vec<MessageDto>, AppError> {
    let mut ids: Vec<UserId> = Vec::new();
    for m in &messages {
        ids.push(m.sender_user_id);
        ids.extend(m.mentions.iter().copied());
    }
    let users = resolve::user_map(&state.user, &state.group, ids).await?;
    Ok(messages
        .iter()
        .map(|m| {
            let sender = resolve::summary_from(&users, m.sender_user_id);
            let mentions: Vec<_> = m
                .mentions
                .iter()
                .map(|u| resolve::summary_from(&users, *u))
                .collect();
            dto::message_dto(m, sender, mentions)
        })
        .collect())
}
