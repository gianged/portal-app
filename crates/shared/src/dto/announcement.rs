use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{
    common::UserSummaryDto,
    ids::{ChannelId, MessageId},
};

/// Announcements are editable only within this many minutes of posting; past
/// that they are immutable. Mirrors `domain::model::EDIT_GRACE`.
pub const EDIT_GRACE_MINUTES: i64 = 15;

/// An announcement is a message with the `is_announcement` flag; it reuses
/// `MessageId` in the domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnouncementDto {
    pub id: MessageId,
    pub channel_id: ChannelId,
    pub sender: UserSummaryDto,
    pub body: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub edited_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Server-computed `within_edit_grace(now)` so the client can hide the edit
    /// affordance without re-deriving the grace window.
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostAnnouncementRequest {
    pub channel_id: ChannelId,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditAnnouncementRequest {
    pub body: String,
}
