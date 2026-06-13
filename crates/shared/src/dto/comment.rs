use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::dto::{common::UserSummaryDto, ids::CommentId};

/// One discussion comment on a request or ticket (the parent is implied by the
/// endpoint the page came from).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentDto {
    pub id: CommentId,
    pub author: UserSummaryDto,
    pub body: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub edited_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Viewer-specific: author && within the 15-minute grace window
    /// (server-derived, like `AnnouncementDto.editable`).
    pub editable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCommentRequest {
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCommentRequest {
    pub body: String,
}
