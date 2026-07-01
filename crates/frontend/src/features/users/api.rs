//! User directory + HR administration; the directory (`list`) also backs the assignee / member / DM pickers across features.

use shared::dto::ids::UserId;
use shared::dto::user::{
    CreateUserRequest, ResetPasswordRequest, UpdateProfileRequest, UserDto, UserProfileDto,
};
use web_sys::js_sys;

use crate::api::client;
use crate::api::error::FrontendError;

/// Active user directory (`GET /users`); `q` filters by name/email substring.
pub async fn list(q: Option<String>) -> Result<Vec<UserDto>, FrontendError> {
    let query = match q {
        Some(term) => {
            let encoded = String::from(js_sys::encode_uri_component(&term));
            client::query(&[("q", &encoded)])
        }
        None => String::new(),
    };
    client::get_json(&format!("/users{query}")).await
}

/// Full profile for one user (`GET /users/{id}`).
pub async fn get(id: UserId) -> Result<UserProfileDto, FrontendError> {
    client::get_json(&format!("/users/{}", id.0)).await
}

/// Provision a new account (HR; `POST /users`).
pub async fn create(req: &CreateUserRequest) -> Result<UserProfileDto, FrontendError> {
    client::post_json("/users", req).await
}

/// Edit a profile (self or HR; `PATCH /users/{id}`).
pub async fn update(
    id: UserId,
    req: &UpdateProfileRequest,
) -> Result<UserProfileDto, FrontendError> {
    client::patch_json(&format!("/users/{}", id.0), req).await
}

/// Deactivate an account (HR; `POST /users/{id}/deactivate`).
pub async fn deactivate(id: UserId) -> Result<(), FrontendError> {
    client::post_no_content(&format!("/users/{}/deactivate", id.0)).await
}

/// Reactivate an account (HR; `POST /users/{id}/reactivate`).
pub async fn reactivate(id: UserId) -> Result<UserProfileDto, FrontendError> {
    client::post_empty(&format!("/users/{}/reactivate", id.0)).await
}

/// Set a temporary password for a user, revoking their sessions (HR; `POST /users/{id}/reset-password`).
pub async fn reset_password(id: UserId, req: &ResetPasswordRequest) -> Result<(), FrontendError> {
    client::post_json_no_content(&format!("/users/{}/reset-password", id.0), req).await
}
