//! Attendance policy HTTP wrappers. `GET /policy` is readable by any user; the
//! `PUT` is HR/Director and rejected server-side otherwise.

use shared::dto::policy::{PolicyDto, UpdatePolicyRequest};

use crate::api::client;
use crate::api::error::FrontendError;

pub async fn get_policy() -> Result<PolicyDto, FrontendError> {
    client::get_json("/policy").await
}

pub async fn update_policy(req: &UpdatePolicyRequest) -> Result<PolicyDto, FrontendError> {
    client::put_json("/policy", req).await
}
