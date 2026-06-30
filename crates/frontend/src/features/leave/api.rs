//! Leave-balance HTTP wrappers. Balance + statement are the signed-in user's own;
//! grant + adjust target a user and are HR-gated server-side.

use shared::dto::ids::UserId;
use shared::dto::leave_balance::{
    AdjustBalanceRequest, LeaveBalanceDto, LeaveStatementDto, SetLeaveGrantRequest,
};

use crate::api::client;
use crate::api::error::FrontendError;

pub async fn my_balance() -> Result<LeaveBalanceDto, FrontendError> {
    client::get_json("/leave/balance").await
}

pub async fn statement(from: &str, to: &str) -> Result<LeaveStatementDto, FrontendError> {
    let q = client::query(&[("from", from), ("to", to)]);
    client::get_json(&format!("/leave/statement{q}")).await
}

pub async fn user_balance(user: UserId) -> Result<LeaveBalanceDto, FrontendError> {
    client::get_json(&format!("/users/{}/leave/balance", user.0)).await
}

pub async fn set_grant(
    user: UserId,
    req: &SetLeaveGrantRequest,
) -> Result<LeaveBalanceDto, FrontendError> {
    client::put_json(&format!("/users/{}/leave/grant", user.0), req).await
}

pub async fn adjust(
    user: UserId,
    req: &AdjustBalanceRequest,
) -> Result<LeaveBalanceDto, FrontendError> {
    client::post_json(&format!("/users/{}/leave/adjust", user.0), req).await
}
