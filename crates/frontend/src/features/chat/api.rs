//! Chat REST wrappers: channel list, DM open, message history + one-shot
//! actions. Live delivery is over the WebSocket ([`crate::api::ws`]); these cover
//! history and the REST fallback for sending/editing.

use serde::Serialize;
use web_sys::FormData;

use shared::dto::chat::{
    ChannelDto, ChannelSummaryDto, ChatAttachmentDto, EditMessageRequest, MessageDto,
    SendMessageRequest,
};
use shared::dto::ids::{ChannelId, MessageId, UserId};

use crate::api::client;
use crate::api::error::FrontendError;

/// The caller's channels (group / general / direct) with unread flags.
pub async fn channels() -> Result<Vec<ChannelSummaryDto>, FrontendError> {
    client::get_json("/chat/channels").await
}

#[derive(Serialize)]
struct OpenDirect {
    user_id: UserId,
}

/// Open (or fetch) the direct channel with another user.
pub async fn open_direct(user: UserId) -> Result<ChannelDto, FrontendError> {
    client::post_json("/chat/direct", &OpenDirect { user_id: user }).await
}

/// A page of message history, newest first. `before` pages backwards.
pub async fn messages(
    channel: ChannelId,
    before: Option<MessageId>,
    limit: u32,
) -> Result<Vec<MessageDto>, FrontendError> {
    let limit_s = limit.to_string();
    let path = match before {
        Some(b) => {
            let before_s = b.0.to_string();
            let q = client::query(&[("before", &before_s), ("limit", &limit_s)]);
            format!("/chat/channels/{}/messages{q}", channel.0)
        }
        None => {
            let q = client::query(&[("limit", &limit_s)]);
            format!("/chat/channels/{}/messages{q}", channel.0)
        }
    };
    client::get_json(&path).await
}

/// Send a message over REST (the WS path is preferred when connected).
pub async fn send(
    channel: ChannelId,
    req: &SendMessageRequest,
) -> Result<MessageDto, FrontendError> {
    client::post_json(&format!("/chat/channels/{}/messages", channel.0), req).await
}

pub async fn edit(
    channel: ChannelId,
    message: MessageId,
    req: &EditMessageRequest,
) -> Result<MessageDto, FrontendError> {
    client::patch_json(
        &format!("/chat/channels/{}/messages/{}", channel.0, message.0),
        req,
    )
    .await
}

pub async fn delete(channel: ChannelId, message: MessageId) -> Result<(), FrontendError> {
    client::del(&format!(
        "/chat/channels/{}/messages/{}",
        channel.0, message.0
    ))
    .await
}

pub async fn mark_read(channel: ChannelId) -> Result<(), FrontendError> {
    client::post_no_content(&format!("/chat/channels/{}/read", channel.0)).await
}

/// Upload one file for a channel (multipart). The returned `storage_key` goes
/// into a subsequent message's `attachment_keys`.
pub async fn upload_attachment(
    channel: ChannelId,
    form: FormData,
) -> Result<ChatAttachmentDto, FrontendError> {
    client::post_multipart(&format!("/chat/channels/{}/attachments", channel.0), form).await
}
