use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{
    common::UserSummaryDto,
    ids::{ChannelId, GroupId, MessageId, UserId},
};

/// Mirrors `domain::model::ChannelKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    Group,
    General,
    Direct,
}

impl ChannelKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Group => "Group",
            Self::General => "General",
            Self::Direct => "Direct",
        }
    }
}

/// Tagged like `domain::model::Channel`, but presentation-shaped: a direct
/// channel exposes the *other* participant (the server resolves it relative to
/// the caller) rather than the stored low/high user pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChannelDto {
    Group {
        id: ChannelId,
        group_id: GroupId,
        name: String,
    },
    General {
        id: ChannelId,
    },
    Direct {
        id: ChannelId,
        other_user: UserSummaryDto,
    },
}

/// Sidebar list item with an unread badge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSummaryDto {
    pub id: ChannelId,
    pub kind: ChannelKind,
    /// Group name, "General", or the other user's name.
    pub title: String,
    pub unread: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_message_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDto {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub sender: UserSummaryDto,
    pub body: String,
    /// Resolved for rendering `@name`.
    pub mentions: Vec<UserSummaryDto>,
    pub attachment_keys: Vec<String>,
    pub is_announcement: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub edited_at: Option<OffsetDateTime>,
    /// Soft delete; when set the UI shows "message deleted".
    #[serde(with = "time::serde::rfc3339::option")]
    pub deleted_at: Option<OffsetDateTime>,
    /// Derived from the message's time-ordered (v7) id by the server.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// Maps to `application::commands::PostMessageCommand` (channel from the path).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub body: String,
    pub mentions: Vec<UserId>,
    pub attachment_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditMessageRequest {
    pub body: String,
}
