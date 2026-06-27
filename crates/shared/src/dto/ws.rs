use serde::{Deserialize, Serialize};

use crate::dto::{
    chat::MessageDto,
    ids::{ChannelId, MessageId, UserId},
};

/// Frames sent from the browser to the server over the chat WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    /// Start receiving live events for a channel.
    Subscribe {
        channel_id: ChannelId,
    },
    Unsubscribe {
        channel_id: ChannelId,
    },
    /// Send a message over the socket (the REST `SendMessageRequest` is the
    /// fallback path).
    SendMessage {
        channel_id: ChannelId,
        body: String,
        mentions: Vec<UserId>,
        attachment_keys: Vec<String>,
    },
    /// Typing indicator.
    Typing {
        channel_id: ChannelId,
    },
    /// Advance this user's read marker.
    MarkRead {
        channel_id: ChannelId,
        up_to: MessageId,
    },
    /// Presence heartbeat; the server replies with [`ServerFrame::Pong`].
    Ping,
}

/// Frames pushed from the server to the browser over the chat WebSocket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerFrame {
    /// Confirms a [`ClientFrame::Subscribe`].
    Subscribed {
        channel_id: ChannelId,
    },
    MessageCreated {
        message: MessageDto,
    },
    MessageEdited {
        message: MessageDto,
    },
    MessageDeleted {
        channel_id: ChannelId,
        message_id: MessageId,
    },
    /// Another participant is typing.
    Typing {
        channel_id: ChannelId,
        user_id: UserId,
    },
    /// A user's online state changed.
    Presence {
        user_id: UserId,
        online: bool,
    },
    /// A read marker moved (multi-device sync).
    ReadMarker {
        channel_id: ChannelId,
        user_id: UserId,
        up_to: MessageId,
    },
    Pong,
    /// Transport-level error inside the socket (distinct from the HTTP
    /// `ApiError` body); structurally identical so the UI can reuse one path.
    Error {
        code: String,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn client_frame_tagged_by_type() {
        let frame = ClientFrame::Subscribe {
            channel_id: ChannelId(Uuid::nil()),
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert!(json.contains("\"type\":\"subscribe\""), "got {json}");
        let back: ClientFrame = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, ClientFrame::Subscribe { .. }));
    }

    #[test]
    fn ping_pong_tags() {
        assert_eq!(
            serde_json::to_string(&ClientFrame::Ping).unwrap(),
            "{\"type\":\"ping\"}"
        );
        assert_eq!(
            serde_json::to_string(&ServerFrame::Pong).unwrap(),
            "{\"type\":\"pong\"}"
        );
    }
}
