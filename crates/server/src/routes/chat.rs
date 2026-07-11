//! Chat REST surface: channel list, history, posting, editing, deletion, read
//! markers. Live delivery is over the WebSocket (`chat_ws`); senders edit their
//! own messages within the grace window, announcements via the announcements routes.

use std::{collections::HashMap, time::Duration};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::StatusCode,
    routing,
};
use futures::future;
use serde::Deserialize;
use uuid::Uuid;

use application::commands::chat::AddChatAttachmentCommand;
use domain::{
    ids::{ChannelId, MessageId, UserId},
    model::{Channel, DirectChannel, Message},
    ports::file_storage::FileStorage,
};
use shared::dto::{
    chat::{
        ChannelDto, ChannelSummaryDto, ChatAttachmentDto, EditMessageRequest, MessageDto,
        OpenDirectRequest, SendMessageRequest,
    },
    ids as wire,
};

use crate::{
    app::AppState,
    dto,
    error::AppError,
    extractors::{app_json::AppJson, auth_user::AuthUser, validated_json::ValidatedJson},
    resolve, routes,
};

/// Chat-attachment upload cap; overrides the global 1 MiB body limit for this route.
const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

/// Lifetime of a presigned attachment download URL handed to clients.
const DOWNLOAD_URL_TTL: Duration = Duration::from_hours(1);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/chat/direct", routing::post(open_direct))
        .route("/chat/channels", routing::get(list_channels))
        .route(
            "/chat/channels/{id}/messages",
            routing::get(list_messages).post(post_message),
        )
        .route(
            "/chat/channels/{id}/messages/{message_id}",
            routing::delete(delete_message).patch(edit_message),
        )
        .route("/chat/channels/{id}/read", routing::post(mark_read))
        .route(
            "/chat/channels/{id}/attachments",
            routing::post(upload_attachment).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
}

#[derive(Deserialize)]
struct HistoryQuery {
    before: Option<Uuid>,
    limit: Option<u32>,
}

async fn open_direct(
    State(state): State<AppState>,
    auth: AuthUser,
    AppJson(body): AppJson<OpenDirectRequest>,
) -> Result<Json<ChannelDto>, AppError> {
    let other = UserId(body.user_id.0);
    let channel = state.chat.open_direct_channel(auth.user_id, other).await?;
    let other_summary = resolve::user_summary(&state.user, &state.group, other).await?;
    Ok(Json(dto::channel_dto(&channel, Some(other_summary))))
}

async fn list_channels(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<ChannelSummaryDto>>, AppError> {
    let overviews = state.chat.channel_overviews(auth.user_id).await?;
    // Per-channel lookups run concurrently; DM counterparts resolve in one batch.
    let channels = future::try_join_all(
        overviews
            .iter()
            .map(|o| state.chat.find_channel(o.membership.channel_id)),
    )
    .await?;
    let counterparts = channels.iter().flatten().filter_map(|c| match c {
        Channel::Direct(c) => Some(dm_counterpart(c, auth.user_id)),
        Channel::Group(_) | Channel::General(_) => None,
    });
    let users = resolve::user_map(&state.user, &state.group, counterparts).await?;

    let mut out = Vec::with_capacity(overviews.len());
    for (overview, channel) in overviews.iter().zip(&channels) {
        let title = match channel {
            // Dangling membership (channel row gone): render a placeholder.
            None => "Channel".to_owned(),
            Some(Channel::Group(c)) => c.name.clone(),
            Some(Channel::General(_)) => "General".to_owned(),
            Some(Channel::Direct(c)) => {
                resolve::summary_from(&users, dm_counterpart(c, auth.user_id)).full_name
            }
        };
        out.push(dto::channel_summary_dto(
            &overview.membership,
            title,
            overview.unread,
            overview.last_message_at,
        ));
    }
    Ok(Json(out))
}

/// The other participant of a direct channel from the viewer's perspective.
fn dm_counterpart(c: &DirectChannel, viewer: UserId) -> UserId {
    if c.user_low_id == viewer {
        c.user_high_id
    } else {
        c.user_low_id
    }
}

async fn list_messages(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::ChannelId>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<MessageDto>>, AppError> {
    let before = q.before.map(MessageId);
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let messages = state
        .chat
        .list_messages(auth.user_id, ChannelId(id.0), before, limit)
        .await?;
    Ok(Json(
        messages_to_dtos(&state, auth.user_id, messages).await?,
    ))
}

async fn post_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::ChannelId>,
    ValidatedJson(body): ValidatedJson<SendMessageRequest>,
) -> Result<Json<MessageDto>, AppError> {
    let message = state
        .chat
        .post_message(
            auth.user_id,
            dto::post_message_command(ChannelId(id.0), body),
        )
        .await?;
    Ok(Json(message_to_dto(&state, auth.user_id, &message).await?))
}

async fn edit_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, message_id)): Path<(wire::ChannelId, wire::MessageId)>,
    ValidatedJson(body): ValidatedJson<EditMessageRequest>,
) -> Result<Json<MessageDto>, AppError> {
    let message = state
        .chat
        .edit_message(
            auth.user_id,
            ChannelId(id.0),
            MessageId(message_id.0),
            body.body,
        )
        .await?;
    Ok(Json(message_to_dto(&state, auth.user_id, &message).await?))
}

async fn delete_message(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, message_id)): Path<(wire::ChannelId, wire::MessageId)>,
) -> Result<StatusCode, AppError> {
    state
        .chat
        .delete_message(auth.user_id, ChannelId(id.0), MessageId(message_id.0))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::ChannelId>,
) -> Result<StatusCode, AppError> {
    state.chat.mark_read(auth.user_id, ChannelId(id.0)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Upload one file for a channel the caller may post in. The returned
/// `storage_key` goes into a subsequent message's `attachment_keys`.
async fn upload_attachment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<wire::ChannelId>,
    mut multipart: Multipart,
) -> Result<Json<ChatAttachmentDto>, AppError> {
    let (filename, content_type, bytes) = routes::read_upload_field(&mut multipart).await?;

    let attachment = state
        .chat
        .add_attachment(
            auth.user_id,
            ChannelId(id.0),
            AddChatAttachmentCommand {
                filename,
                content_type,
                bytes,
            },
        )
        .await?;
    let download_url = state
        .storage
        .presign_get(&attachment.storage_key, DOWNLOAD_URL_TTL, auth.user_id)
        .await
        .map_err(|e| AppError::Domain(application::Error::Storage(e)))?;
    Ok(Json(dto::chat_attachment_dto(&attachment, download_url)))
}

/// Resolves + presigns the attachments referenced by a set of messages, keyed
/// by storage key. Deleted messages contribute nothing.
async fn attachments_map(
    state: &AppState,
    viewer: UserId,
    messages: &[&Message],
) -> Result<HashMap<String, ChatAttachmentDto>, AppError> {
    let keys: Vec<String> = messages
        .iter()
        .filter(|m| !m.is_deleted())
        .flat_map(|m| m.attachment_keys.iter().cloned())
        .collect();
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let attachments = state.chat.attachments_by_keys(&keys).await?;
    let mut map = HashMap::with_capacity(attachments.len());
    for a in &attachments {
        let download_url = state
            .storage
            .presign_get(&a.storage_key, DOWNLOAD_URL_TTL, viewer)
            .await
            .map_err(|e| AppError::Domain(application::Error::Storage(e)))?;
        map.insert(
            a.storage_key.clone(),
            dto::chat_attachment_dto(a, download_url),
        );
    }
    Ok(map)
}

/// The message's attachment DTOs from a prebuilt map (deleted messages render
/// none; keys with no metadata row are skipped rather than invented).
fn attachments_for(
    message: &Message,
    map: &HashMap<String, ChatAttachmentDto>,
) -> Vec<ChatAttachmentDto> {
    if message.is_deleted() {
        return Vec::new();
    }
    message
        .attachment_keys
        .iter()
        .filter_map(|k| map.get(k).cloned())
        .collect()
}

/// Resolves one message's sender + mention summaries and its attachments
/// (presigned for `viewer`).
async fn message_to_dto(
    state: &AppState,
    viewer: UserId,
    message: &Message,
) -> Result<MessageDto, AppError> {
    let sender = resolve::user_summary(&state.user, &state.group, message.sender_user_id).await?;
    let mut mentions = Vec::with_capacity(message.mentions.len());
    for uid in &message.mentions {
        mentions.push(resolve::user_summary(&state.user, &state.group, *uid).await?);
    }
    let map = attachments_map(state, viewer, &[message]).await?;
    let attachments = attachments_for(message, &map);
    Ok(dto::message_dto(message, sender, mentions, attachments))
}

/// Resolves a page of messages, deduplicating sender + mention + attachment
/// lookups.
async fn messages_to_dtos(
    state: &AppState,
    viewer: UserId,
    messages: Vec<Message>,
) -> Result<Vec<MessageDto>, AppError> {
    let mut ids: Vec<UserId> = Vec::new();
    for m in &messages {
        ids.push(m.sender_user_id);
        ids.extend(m.mentions.iter().copied());
    }
    let users = resolve::user_map(&state.user, &state.group, ids).await?;
    let refs: Vec<&Message> = messages.iter().collect();
    let map = attachments_map(state, viewer, &refs).await?;
    Ok(messages
        .iter()
        .map(|m| {
            let sender = resolve::summary_from(&users, m.sender_user_id);
            let mentions: Vec<_> = m
                .mentions
                .iter()
                .map(|u| resolve::summary_from(&users, *u))
                .collect();
            dto::message_dto(m, sender, mentions, attachments_for(m, &map))
        })
        .collect())
}
