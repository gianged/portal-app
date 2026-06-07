use shared::dto::user::{LoginRequest, LoginResponse, UserDto};

use crate::api::client;
use crate::api::error::FrontendError;

pub async fn login(req: LoginRequest) -> Result<LoginResponse, FrontendError> {
    client::post_json("/login", &req).await
}

pub async fn logout() -> Result<(), FrontendError> {
    let _: serde_json::Value = client::post_json("/logout", &serde_json::json!({})).await?;
    Ok(())
}

pub async fn me() -> Result<UserDto, FrontendError> {
    client::get_json("/me").await
}
