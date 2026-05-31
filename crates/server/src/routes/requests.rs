//! Work-request endpoints. Listing is scope-filtered via query params;
//! `GET /requests/{id}` returns the request plus its attachments, and
//! `PATCH /requests/{id}` edits metadata (creator-only, before work starts).

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    routing::{get, post},
};
use serde::Deserialize;
use uuid::Uuid;

use application::commands::request::AddAttachmentCommand;
use domain::{
    ids::{ProjectId, RequestId, UserId},
    model::Request,
};
use shared::dto::request::{
    AssignRequestRequest, CreateRequestRequest, RequestAttachmentDto, RequestDetailDto, RequestDto,
    RequestStatus as WireRequestStatus, UpdateRequestRequest,
};
use shared::validation::request::{validate_request_description, validate_request_title};

use crate::{app::AppState, dto, error::AppError, extractors::auth_user::AuthUser, resolve};

/// Upload cap for a single attachment (the default axum body limit is 2 MiB).
const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/requests", post(create).get(list))
        .route("/requests/{id}", get(detail).patch(update))
        .route("/requests/{id}/submit", post(submit))
        .route("/requests/{id}/assign", post(assign))
        .route("/requests/{id}/start", post(start))
        .route("/requests/{id}/review", post(review))
        .route("/requests/{id}/approve", post(approve))
        .route("/requests/{id}/reject", post(reject))
        .route("/requests/{id}/cancel", post(cancel))
        .route(
            "/requests/{id}/attachments",
            post(add_attachment).layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES)),
        )
}

#[derive(Deserialize)]
struct ListQuery {
    /// List requests within this project (requires project view access).
    project: Option<Uuid>,
    /// List requests assigned to the caller.
    #[serde(default)]
    mine: bool,
    /// Optional status filter.
    status: Option<WireRequestStatus>,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateRequestRequest>,
) -> Result<Json<RequestDto>, AppError> {
    validate_request_title(&body.title).map_err(|e| AppError::Validation(e.to_string()))?;
    validate_request_description(&body.description)
        .map_err(|e| AppError::Validation(e.to_string()))?;
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
    let requests = if let Some(project) = q.project {
        state
            .request
            .list_for_project(auth.user_id, ProjectId(project), status)
            .await?
    } else if q.mine {
        state
            .request
            .list_for_assignee(auth.user_id, status)
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
    let attachment_dtos = attachments
        .iter()
        .map(|a| {
            dto::request_attachment_dto(a, resolve::summary_from(&users, a.uploaded_by_user_id))
        })
        .collect();

    Ok(Json(RequestDetailDto {
        request: single(&state, &request).await?,
        attachments: attachment_dtos,
    }))
}

async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRequestRequest>,
) -> Result<Json<RequestDto>, AppError> {
    if let Some(title) = &body.title {
        validate_request_title(title).map_err(|e| AppError::Validation(e.to_string()))?;
    }
    if let Some(description) = &body.description {
        validate_request_description(description)
            .map_err(|e| AppError::Validation(e.to_string()))?;
    }
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

async fn add_attachment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<RequestAttachmentDto>, AppError> {
    let field = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("invalid multipart body: {e}")))?
        .ok_or_else(|| AppError::Validation("no file field in upload".into()))?;
    let filename = field
        .file_name()
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::Validation("upload field has no filename".into()))?;
    let content_type = field
        .content_type()
        .map_or_else(|| "application/octet-stream".to_owned(), ToOwned::to_owned);
    let bytes = field
        .bytes()
        .await
        .map_err(|e| AppError::Validation(format!("reading upload failed: {e}")))?
        .to_vec();

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
    Ok(Json(dto::request_attachment_dto(&attachment, uploader)))
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
