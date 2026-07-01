//! Attendance policy HTTP wrappers; `PUT` is HR/Director-gated server-side.

use shared::dto::policy::{PolicyDto, UpdatePolicyRequest};

use crate::api::client;
use crate::api::error::FrontendError;

pub async fn get_policy() -> Result<PolicyDto, FrontendError> {
    client::get_json("/policy").await
}

pub async fn update_policy(req: &UpdatePolicyRequest) -> Result<PolicyDto, FrontendError> {
    client::put_json("/policy", req).await
}
