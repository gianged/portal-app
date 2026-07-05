//! Work-request endpoints. Listing is scope-filtered via query params;
//! `GET /requests/{id}` returns the request plus its attachments, and
//! `PATCH /requests/{id}` edits metadata (creator-only, before work starts).

use std::time::Duration;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::StatusCode,
    routing,
};
use serde::Deserialize;
use time::OffsetDateTime;
use uuid::Uuid;

use application::commands::request::AddAttachmentCommand;
use domain::{
    ids::{CommentId, ProjectId, RequestId, UserId},
    model::{Comment, CommentEntity, Request},
    ports::file_storage::FileStorage,
};
use shared::dto::{
    comment::{CommentDto, CreateCommentRequest, UpdateCommentRequest},
    request::{
        AssignRequestRequest, CreateRequestRequest, RequestAttachmentDto, RequestDetailDto,
        RequestDto, RequestStatus, SetRequestProgressRequest, UpdateRequestRequest,
    },
};

use crate::{
    app::AppState,
    dto,
    error::AppError,
    extractors::{auth_user::AuthUser, validated_json::ValidatedJson},
    resolve, routes,
};

/// Upload cap for a single attachment (the default axum body limit is 2 MiB).
const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

/// Lifetime of a presigned attachment download URL handed to clients.
const DOWNLOAD_URL_TTL: Duration = Duration::from_hours(1);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/requests", routing::post(create).get(list))
        .route("/requests/{id}", routing::get(detail).patch(update))
        .route("/requests/{id}/submit", routing::post(submit))
        .route("/requests/{id}/assign", routing::post(assign))
        .route("/requests/{id}/start", routing::post(start))
        .route("/requests/{id}/review", routing::post(review))
        .route("/requests/{id}/approve", routing::post(approve))
        .route("/requests/{id}/reject", routing::post(reject))
        .route("/requests/{id}/cancel", routing::post(cancel))
        .route("/requests/{id}/progress", routing::post(set_progress))
        .route(
            "/requests/{id}/attachments",
            routing::post(add_attachment).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
        .route(
            "/requests/{id}/comments",
            routing::get(list_comments).post(add_comment),
        )
        .route(
            "/requests/{id}/comments/{comment_id}",
            routing::patch(edit_comment).delete(delete_comment),
        )
}

#[derive(Deserialize)]
struct CommentsQuery {
    /// Exclusive newest-first cursor (a comment id).
    before: Option<Uuid>,
    limit: Option<u32>,
}

async fn list_comments(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<CommentsQuery>,
) -> Result<Json<Vec<CommentDto>>, AppError> {
    let entity = CommentEntity::Request {
        request_id: RequestId(id),
    };
    let before = q.before.map(CommentId);
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let comments = state
        .comment
        .list(auth.user_id, entity, before, limit)
        .await?;
    comments_to_dtos(&state, auth.user_id, comments).await
}

async fn add_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(body): ValidatedJson<CreateCommentRequest>,
) -> Result<Json<CommentDto>, AppError> {
    let entity = CommentEntity::Request {
        request_id: RequestId(id),
    };
    let comment = state.comment.add(auth.user_id, entity, body.body).await?;
    comment_to_dto(&state, auth.user_id, &comment).await
}

async fn edit_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, comment_id)): Path<(Uuid, Uuid)>,
    ValidatedJson(body): ValidatedJson<UpdateCommentRequest>,
) -> Result<Json<CommentDto>, AppError> {
    let entity = CommentEntity::Request {
        request_id: RequestId(id),
    };
    let comment = state
        .comment
        .edit(auth.user_id, entity, CommentId(comment_id), body.body)
        .await?;
    comment_to_dto(&state, auth.user_id, &comment).await
}

async fn delete_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {
    let entity = CommentEntity::Request {
        request_id: RequestId(id),
    };
    state
        .comment
        .remove(auth.user_id, entity, CommentId(comment_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Resolves one comment's author summary.
async fn comment_to_dto(
    state: &AppState,
    viewer: UserId,
    comment: &Comment,
) -> Result<Json<CommentDto>, AppError> {
    let author = resolve::user_summary(&state.user, &state.group, comment.author_user_id).await?;
    let now = OffsetDateTime::now_utc();
    Ok(Json(dto::comment_dto(comment, author, viewer, now)))
}

/// Resolves a page of comments with one deduped author lookup.
async fn comments_to_dtos(
    state: &AppState,
    viewer: UserId,
    comments: Vec<Comment>,
) -> Result<Json<Vec<CommentDto>>, AppError> {
    let authors = resolve::user_map(
        &state.user,
        &state.group,
        comments.iter().map(|c| c.author_user_id),
    )
    .await?;
    let now = OffsetDateTime::now_utc();
    Ok(Json(
        comments
            .iter()
            .map(|c| {
                let author = resolve::summary_from(&authors, c.author_user_id);
                dto::comment_dto(c, author, viewer, now)
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
struct ListQuery {
    /// List requests within this project (requires project view access).
    project: Option<Uuid>,
    /// List requests assigned to the caller.
    #[serde(default)]
    mine: bool,
    /// Optional status filter.
    status: Option<RequestStatus>,
    /// Substring search on the request title.
    q: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    ValidatedJson(body): ValidatedJson<CreateRequestRequest>,
) -> Result<Json<RequestDto>, AppError> {
    let request = state
        .request
        .create(auth.user_id, dto::create_request_command(body))
        .await?;
    Ok(Json(single(&state, &request).await?))
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<RequestDto>>, AppError> {
    let status = q.status.map(dto::request_status_domain);
    let search = routes::norm_q(q.q);
    let search = search.as_deref();
    let requests = if let Some(project) = q.project {
        state
            .request
            .list_for_project(auth.user_id, ProjectId(project), status, search)
            .await?
    } else if q.mine {
        state
            .request
            .list_for_assignee(auth.user_id, status, search)
            .await?
    } else {
        return Err(AppError::Validation(
            "either project or mine=true query parameter is required".into(),
        ));
    };
    Ok(Json(many(&state, requests).await?))
}

async fn detail(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<RequestDetailDto>, AppError> {
    let rid = RequestId(id);
    let request = state.request.find(auth.user_id, rid).await?;
    let attachments = state.request.list_attachments(auth.user_id, rid).await?;

    let uploader_ids: Vec<UserId> = attachments.iter().map(|a| a.uploaded_by_user_id).collect();
    let users = resolve::user_map(&state.user, &state.group, uploader_ids).await?;
    let mut attachment_dtos = Vec::with_capacity(attachments.len());
    for a in &attachments {
        let download_url = state
            .storage
            .presign_get(&a.storage_key, DOWNLOAD_URL_TTL, auth.user_id)
            .await
            .map_err(|e| AppError::Domain(application::Error::Storage(e)))?;
        attachment_dtos.push(dto::request_attachment_dto(
            a,
            resolve::summary_from(&users, a.uploaded_by_user_id),
            download_url,
        ));
    }

    Ok(Json(RequestDetailDto {
        request: single(&state, &request).await?,
        attachments: attachment_dtos,
    }))
}

async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(body): ValidatedJson<UpdateRequestRequest>,
) -> Result<Json<RequestDto>, AppError> {
    let request = state
        .request
        .update_metadata(
            auth.user_id,
            RequestId(id),
            dto::update_request_command(body),
        )
        .await?;
    Ok(Json(single(&state, &request).await?))
}

async fn submit(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<RequestDto>, AppError> {
    let request = state.request.submit(auth.user_id, RequestId(id)).await?;
    Ok(Json(single(&state, &request).await?))
}

async fn assign(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AssignRequestRequest>,
) -> Result<Json<RequestDto>, AppError> {
    let request = state
        .request
        .assign(auth.user_id, RequestId(id), UserId(body.assignee_user_id.0))
        .await?;
    Ok(Json(single(&state, &request).await?))
}

async fn start(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<RequestDto>, AppError> {
    let request = state.request.start(auth.user_id, RequestId(id)).await?;
    Ok(Json(single(&state, &request).await?))
}

async fn review(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<RequestDto>, AppError> {
    let request = state
        .request
        .send_for_review(auth.user_id, RequestId(id))
        .await?;
    Ok(Json(single(&state, &request).await?))
}

async fn approve(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<RequestDto>, AppError> {
    let request = state.request.approve(auth.user_id, RequestId(id)).await?;
    Ok(Json(single(&state, &request).await?))
}

async fn reject(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<RequestDto>, AppError> {
    let request = state.request.reject(auth.user_id, RequestId(id)).await?;
    Ok(Json(single(&state, &request).await?))
}

async fn cancel(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<RequestDto>, AppError> {
    let request = state.request.cancel(auth.user_id, RequestId(id)).await?;
    Ok(Json(single(&state, &request).await?))
}

async fn set_progress(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    ValidatedJson(body): ValidatedJson<SetRequestProgressRequest>,
) -> Result<Json<RequestDto>, AppError> {
    let request = state
        .request
        .set_progress(auth.user_id, RequestId(id), body.progress)
        .await?;
    Ok(Json(single(&state, &request).await?))
}

async fn add_attachment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<RequestAttachmentDto>, AppError> {
    let (filename, content_type, bytes) = routes::read_upload_field(&mut multipart).await?;

    let attachment = state
        .request
        .add_attachment(
            auth.user_id,
            RequestId(id),
            AddAttachmentCommand {
                filename,
                content_type,
                bytes,
            },
        )
        .await?;
    let uploader =
        resolve::user_summary(&state.user, &state.group, attachment.uploaded_by_user_id).await?;
    let download_url = state
        .storage
        .presign_get(&attachment.storage_key, DOWNLOAD_URL_TTL, auth.user_id)
        .await
        .map_err(|e| AppError::Domain(application::Error::Storage(e)))?;
    Ok(Json(dto::request_attachment_dto(
        &attachment,
        uploader,
        download_url,
    )))
}

/// Resolves one request's creator + assignee summaries.
async fn single(state: &AppState, request: &Request) -> Result<RequestDto, AppError> {
    let creator = resolve::user_summary(&state.user, &state.group, request.creator_user_id).await?;
    let assignee =
        resolve::opt_user_summary(&state.user, &state.group, request.assignee_user_id).await?;
    Ok(dto::request_dto(request, creator, assignee))
}

/// Resolves a batch of requests, deduplicating user lookups.
async fn many(state: &AppState, requests: Vec<Request>) -> Result<Vec<RequestDto>, AppError> {
    let mut ids: Vec<UserId> = Vec::with_capacity(requests.len() * 2);
    for r in &requests {
        ids.push(r.creator_user_id);
        if let Some(assignee) = r.assignee_user_id {
            ids.push(assignee);
        }
    }
    let users = resolve::user_map(&state.user, &state.group, ids).await?;
    Ok(requests
        .iter()
        .map(|r| {
            let creator = resolve::summary_from(&users, r.creator_user_id);
            let assignee = r.assignee_user_id.map(|a| resolve::summary_from(&users, a));
            dto::request_dto(r, creator, assignee)
        })
        .collect())
}
