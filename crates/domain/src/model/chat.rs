use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::ids::{ChannelId, ChatAttachmentId, GroupId, MessageId, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Channel {
    Group(GroupChannel),
    General(GeneralChannel),
    Direct(DirectChannel),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupChannel {
    pub id: ChannelId,
    pub group_id: GroupId,
    pub name: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralChannel {
    pub id: ChannelId,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectChannel {
    pub id: ChannelId,
    pub user_low_id: UserId,
    pub user_high_id: UserId,
    pub created_at: OffsetDateTime,
}

impl DirectChannel {
    /// Sorts the user pair so (A, B) and (B, A) canonicalise to the same row,
    /// matching the `direct_channel_by_users` lookup table.
    #[must_use]
    pub fn new(id: ChannelId, a: UserId, b: UserId, created_at: OffsetDateTime) -> Self {
        let (low, high) = if a.0 <= b.0 { (a, b) } else { (b, a) };
        Self {
            id,
            user_low_id: low,
            user_high_id: high,
            created_at,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    Group,
    General,
    Direct,
}

impl Channel {
    #[must_use]
    pub const fn id(&self) -> ChannelId {
        match self {
            Self::Group(c) => c.id,
            Self::General(c) => c.id,
            Self::Direct(c) => c.id,
        }
    }

    #[must_use]
    pub const fn created_at(&self) -> OffsetDateTime {
        match self {
            Self::Group(c) => c.created_at,
            Self::General(c) => c.created_at,
            Self::Direct(c) => c.created_at,
        }
    }
}

/// Postgres metadata for a chat upload (the Scylla row keeps only `attachment_keys`);
/// mirrors [`super::request::RequestAttachment`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatAttachment {
    pub id: ChatAttachmentId,
    pub channel_id: ChannelId,
    pub uploaded_by_user_id: UserId,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub storage_key: String,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub sender_user_id: UserId,
    pub body: String,
    pub mentions: Vec<UserId>,
    pub attachment_keys: Vec<String>,
    pub is_announcement: bool,
    pub edited_at: Option<OffsetDateTime>,
    pub deleted_at: Option<OffsetDateTime>,
}

impl Message {
    #[must_use]
    pub const fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }

    pub fn edit(&mut self, body: String, now: OffsetDateTime) {
        self.body = body;
        self.edited_at = Some(now);
    }

    /// Soft delete: body is retained for audit / moderation per the
    /// `messages_by_channel` schema comment.
    pub fn delete(&mut self, now: OffsetDateTime) {
        self.deleted_at = Some(now);
    }
}

/// Mirrors a row of Cassandra's `channels_by_user`: channels a user can see plus
/// their read marker for unread-badge computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMembership {
    pub user_id: UserId,
    pub channel_id: ChannelId,
    pub kind: ChannelKind,
    pub last_read_at: Option<OffsetDateTime>,
}
