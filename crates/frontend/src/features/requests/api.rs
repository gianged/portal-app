//! Work-request HTTP wrappers. Listing is scope-filtered (`mine` or `project`);
//! the lifecycle endpoints each return the updated [`RequestDto`].

use web_sys::FormData;

use shared::dto::ids::{ProjectId, RequestId};
use shared::dto::request::{
    AssignRequestRequest, CreateRequestRequest, RequestAttachmentDto, RequestDetailDto, RequestDto,
    RequestStatus, UpdateRequestRequest,
};

use crate::api::client;
use crate::api::error::FrontendError;

/// Wire (`snake_case`) value for the `status` query filter.
fn status_param(status: RequestStatus) -> &'static str {
    match status {
        RequestStatus::Draft => "draft",
        RequestStatus::Submitted => "submitted",
        RequestStatus::Assigned => "assigned",
        RequestStatus::InProgress => "in_progress",
        RequestStatus::Review => "review",
        RequestStatus::Completed => "completed",
        RequestStatus::Cancelled => "cancelled",
    }
}

/// Requests assigned to the caller (`GET /requests?mine=true`); `q` filters by
/// title substring. Owned `q` so the future is `'static` for the `load` helper.
pub async fn list_mine(
    status: Option<RequestStatus>,
    q: Option<String>,
) -> Result<Vec<RequestDto>, FrontendError> {
    let mut pairs: Vec<(&str, &str)> = vec![("mine", "true")];
    if let Some(s) = status {
        pairs.push(("status", status_param(s)));
    }
    let encoded = q.map(|term| String::from(web_sys::js_sys::encode_uri_component(&term)));
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
        Some(s) => client::query(&[("project", &pid), ("status", status_param(s))]),
        None => client::query(&[("project", &pid)]),
    };
    client::get_json(&format!("/requests{q}")).await
}

/// One request plus its attachments (`GET /requests/{id}`).
pub async fn get(id: RequestId) -> Result<RequestDetailDto, FrontendError> {
    client::get_json(&format!("/requests/{}", id.0)).await
}

pub async fn create(req: &CreateRequestRequest) -> Result<RequestDto, FrontendError> {
    client::post_json("/requests", req).await
}

#[allow(dead_code)] // TODO: unused, I will see it
pub async fn update(
    id: RequestId,
    req: &UpdateRequestRequest,
) -> Result<RequestDto, FrontendError> {
    client::patch_json(&format!("/requests/{}", id.0), req).await
}

pub async fn submit(id: RequestId) -> Result<RequestDto, FrontendError> {
    client::post_empty(&format!("/requests/{}/submit", id.0)).await
}

pub async fn assign(
    id: RequestId,
    req: &AssignRequestRequest,
) -> Result<RequestDto, FrontendError> {
    client::post_json(&format!("/requests/{}/assign", id.0), req).await
}

pub async fn start(id: RequestId) -> Result<RequestDto, FrontendError> {
    client::post_empty(&format!("/requests/{}/start", id.0)).await
}

pub async fn send_for_review(id: RequestId) -> Result<RequestDto, FrontendError> {
    client::post_empty(&format!("/requests/{}/review", id.0)).await
}

pub async fn approve(id: RequestId) -> Result<RequestDto, FrontendError> {
    client::post_empty(&format!("/requests/{}/approve", id.0)).await
}

pub async fn reject(id: RequestId) -> Result<RequestDto, FrontendError> {
    client::post_empty(&format!("/requests/{}/reject", id.0)).await
}

pub async fn cancel(id: RequestId) -> Result<RequestDto, FrontendError> {
    client::post_empty(&format!("/requests/{}/cancel", id.0)).await
}

/// Upload one attachment (`POST /requests/{id}/attachments`, multipart).
pub async fn upload_attachment(
    id: RequestId,
    form: FormData,
) -> Result<RequestAttachmentDto, FrontendError> {
    client::post_multipart(&format!("/requests/{}/attachments", id.0), form).await
}
