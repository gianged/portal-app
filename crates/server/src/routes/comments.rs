//! Shared comment sub-resource logic for requests and tickets: same
//! `CommentService`, same wire DTO, parameterized by the owning [`CommentEntity`]
//! so the two route modules cannot drift.

use serde::Deserialize;
use time::OffsetDateTime;
use uuid::Uuid;

use domain::{
    ids::{CommentId, UserId},
    model::{Comment, CommentEntity},
};
use shared::dto::comment::CommentDto;

use crate::{app::AppState, dto, error::AppError, resolve};

#[derive(Deserialize)]
pub(crate) struct CommentsQuery {
    /// Exclusive newest-first cursor (a comment id).
    before: Option<Uuid>,
    limit: Option<u32>,
}

/// Lists a page of comments on `entity`.
pub(crate) async fn list(
    state: &AppState,
    viewer: UserId,
    entity: CommentEntity,
    q: &CommentsQuery,
) -> Result<Vec<CommentDto>, AppError> {
    let before = q.before.map(CommentId);
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let comments = state.comment.list(viewer, entity, before, limit).await?;
    many(state, viewer, comments).await
}

/// Adds a comment to `entity`.
pub(crate) async fn add(
    state: &AppState,
    viewer: UserId,
    entity: CommentEntity,
    body: String,
) -> Result<CommentDto, AppError> {
    let comment = state.comment.add(viewer, entity, body).await?;
    single(state, viewer, &comment).await
}

/// Edits one comment on `entity`.
pub(crate) async fn edit(
    state: &AppState,
    viewer: UserId,
    entity: CommentEntity,
    comment_id: CommentId,
    body: String,
) -> Result<CommentDto, AppError> {
    let comment = state.comment.edit(viewer, entity, comment_id, body).await?;
    single(state, viewer, &comment).await
}

/// Removes one comment from `entity`.
pub(crate) async fn remove(
    state: &AppState,
    viewer: UserId,
    entity: CommentEntity,
    comment_id: CommentId,
) -> Result<(), AppError> {
    state.comment.remove(viewer, entity, comment_id).await?;
    Ok(())
}

/// Resolves one comment's author summary.
async fn single(
    state: &AppState,
    viewer: UserId,
    comment: &Comment,
) -> Result<CommentDto, AppError> {
    let author = resolve::user_summary(&state.user, &state.group, comment.author_user_id).await?;
    let now = OffsetDateTime::now_utc();
    Ok(dto::comment_dto(comment, author, viewer, now))
}

/// Resolves a page of comments with one deduped author lookup.
async fn many(
    state: &AppState,
    viewer: UserId,
    comments: Vec<Comment>,
) -> Result<Vec<CommentDto>, AppError> {
    let authors = resolve::user_map(
        &state.user,
        &state.group,
        comments.iter().map(|c| c.author_user_id),
    )
    .await?;
    let now = OffsetDateTime::now_utc();
    Ok(comments
        .iter()
        .map(|c| {
            let author = resolve::summary_from(&authors, c.author_user_id);
            dto::comment_dto(c, author, viewer, now)
        })
        .collect())
}
