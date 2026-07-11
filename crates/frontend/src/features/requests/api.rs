//! Work-request HTTP wrappers; listing is scope-filtered (`mine` or `project`), and the lifecycle endpoints each return the updated [`RequestDto`].

use web_sys::{FormData, js_sys};

use shared::dto::ids::{ProjectId, RequestId};
use shared::dto::request::{
    AssignRequestRequest, CreateRequestRequest, RequestAttachmentDto, RequestDetailDto, RequestDto,
    RequestStatus, SetRequestProgressRequest, UpdateRequestRequest,
};

use crate::api::client;
use crate::api::error::FrontendError;

/// Requests assigned to the caller (`GET /requests?mine=true`); `q` filters by title substring.
pub async fn list_mine(
    status: Option<RequestStatus>,
    q: Option<String>,
) -> Result<Vec<RequestDto>, FrontendError> {
    let mut pairs: Vec<(&str, &str)> = vec![("mine", "true")];
    if let Some(s) = status {
        pairs.push(("status", s.as_str()));
    }
    let encoded = q.map(|term| String::from(js_sys::encode_uri_component(&term)));
    if let Some(encoded) = &encoded {
        pairs.push(("q", encoded));
    }
    let query = client::query(&pairs);
    client::get_json(&format!("/requests{query}")).await
}

/// Requests within a project (`GET /requests?project=…`).
pub async fn list_for_project(
    project: ProjectId,
    status: Option<RequestStatus>,
) -> Result<Vec<RequestDto>, FrontendError> {
    let pid = project.0.to_string();
    let q = match status {
        Some(s) => client::query(&[("project", &pid), ("status", s.as_str())]),
        None => client::query(&[("project", &pid)]),
    };
    client::get_json(&format!("/requests{q}")).await
}

/// One request plus its attachments (`GET /requests/{id}`).
pub async fn get(id: RequestId) -> Result<RequestDetailDto, FrontendError> {
    client::get_json(&format!("/requests/{}", id.0)).await
}

/// Create a new draft request.
pub async fn create(req: &CreateRequestRequest) -> Result<RequestDto, FrontendError> {
    client::post_json("/requests", req).await
}

/// Update a draft request's fields.
#[allow(dead_code)]
pub async fn update(
    id: RequestId,
    req: &UpdateRequestRequest,
) -> Result<RequestDto, FrontendError> {
    client::patch_json(&format!("/requests/{}", id.0), req).await
}

/// Submit a draft for assignment.
pub async fn submit(id: RequestId) -> Result<RequestDto, FrontendError> {
    client::post_empty(&format!("/requests/{}/submit", id.0)).await
}

/// Assign a submitted request to a user.
pub async fn assign(
    id: RequestId,
    req: &AssignRequestRequest,
) -> Result<RequestDto, FrontendError> {
    client::post_json(&format!("/requests/{}/assign", id.0), req).await
}

/// Start work on an assigned request.
pub async fn start(id: RequestId) -> Result<RequestDto, FrontendError> {
    client::post_empty(&format!("/requests/{}/start", id.0)).await
}

/// Move an in-progress request to review.
pub async fn send_for_review(id: RequestId) -> Result<RequestDto, FrontendError> {
    client::post_empty(&format!("/requests/{}/review", id.0)).await
}

/// Approve a request in review, completing it.
pub async fn approve(id: RequestId) -> Result<RequestDto, FrontendError> {
    client::post_empty(&format!("/requests/{}/approve", id.0)).await
}

/// Reject a review, sending the request back to in progress.
pub async fn reject(id: RequestId) -> Result<RequestDto, FrontendError> {
    client::post_empty(&format!("/requests/{}/reject", id.0)).await
}

/// Cancel a request before completion.
pub async fn cancel(id: RequestId) -> Result<RequestDto, FrontendError> {
    client::post_empty(&format!("/requests/{}/cancel", id.0)).await
}

/// Set the assignee-reported completion percentage (assignee, while in progress).
pub async fn set_progress(id: RequestId, progress: u8) -> Result<RequestDto, FrontendError> {
    let req = SetRequestProgressRequest { progress };
    client::post_json(&format!("/requests/{}/progress", id.0), &req).await
}

/// Upload one attachment (`POST /requests/{id}/attachments`, multipart).
pub async fn upload_attachment(
    id: RequestId,
    form: FormData,
) -> Result<RequestAttachmentDto, FrontendError> {
    client::post_multipart(&format!("/requests/{}/attachments", id.0), form).await
}
