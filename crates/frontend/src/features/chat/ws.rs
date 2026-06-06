//! Projection of incoming [`ServerFrame`]s onto a channel's message list +
//! typing state. The single [`WsClient`](crate::api::ws::WsClient) delivers every
//! frame; the thread filters to its channel and folds the relevant ones in.

use std::time::Duration;

use leptos::prelude::*;

use shared::dto::chat::MessageDto;
use shared::dto::ids::ChannelId;
use shared::dto::ws::ServerFrame;

/// Insert or replace a message by id (dedupes the WS echo against an optimistic
/// REST insert).
pub fn push_message(messages: RwSignal<Vec<MessageDto>>, msg: MessageDto) {
    messages.update(|v| {
        if let Some(existing) = v.iter_mut().find(|m| m.id == msg.id) {
            *existing = msg;
        } else {
            v.push(msg);
        }
    });
}

/// Fold one server frame into the open channel's view. Frames for other channels
/// are ignored.
pub fn apply_server_frame(
    frame: &ServerFrame,
    channel: ChannelId,
    messages: RwSignal<Vec<MessageDto>>,
    typing: RwSignal<bool>,
) {
    match frame {
        ServerFrame::MessageCreated { message } | ServerFrame::MessageEdited { message }
            if message.channel_id == channel =>
        {
            push_message(messages, message.clone());
        }
        ServerFrame::MessageDeleted {
            channel_id,
            message_id,
        } if *channel_id == channel => {
            messages.update(|v| v.retain(|m| m.id != *message_id));
        }
        ServerFrame::Typing { channel_id, .. } if *channel_id == channel => {
            typing.set(true);
            set_timeout(move || typing.set(false), Duration::from_secs(3));
        }
        _ => {}
    }
}
