//! Domain <-> wire projection for work-item comments.

use domain::{ids::UserId, model};
use shared::dto::{comment::CommentDto, common::UserSummaryDto};
use time::OffsetDateTime;

/// `editable` is viewer-specific: the author within the grace window (mirrors
/// `announcement_dto`).
#[must_use]
pub fn comment_dto(
    comment: &model::Comment,
    author: UserSummaryDto,
    viewer: UserId,
    now: OffsetDateTime,
) -> CommentDto {
    CommentDto {
        id: super::comment_id(comment.id),
        author,
        body: comment.body.clone(),
        edited_at: comment.edited_at,
        created_at: comment.created_at,
        editable: comment.author_user_id == viewer && comment.within_edit_grace(now),
    }
}
