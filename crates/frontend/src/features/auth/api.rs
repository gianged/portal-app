use shared::dto::user::{ChangePasswordRequest, LoginRequest, LoginResponse, UserDto};

use crate::api::client;
use crate::api::error::FrontendError;

pub async fn login(req: LoginRequest) -> Result<LoginResponse, FrontendError> {
    client::post_json("/login", &req).await
}

pub async fn logout() -> Result<(), FrontendError> {
    client::post_no_content("/logout").await
}

pub async fn me() -> Result<UserDto, FrontendError> {
    client::get_json("/me").await
}

/// Self-service password change (`POST /me/password`); success revokes all sessions including the current one, so the caller must re-login.
pub async fn change_password(req: &ChangePasswordRequest) -> Result<(), FrontendError> {
    client::post_json_no_content("/me/password", req).await
}
