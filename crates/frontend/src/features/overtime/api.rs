//! Overtime HTTP wrappers. `work_date` is `"YYYY-MM-DD"`; queues and decisions
//! are leader/HR-gated server-side.

use shared::dto::ids::{GroupId, OvertimeId};
use shared::dto::overtime::{CreateOvertimeRequest, DecideOvertimeRequest, OvertimeDto};

use crate::api::client;
use crate::api::error::FrontendError;

pub async fn create(req: &CreateOvertimeRequest) -> Result<OvertimeDto, FrontendError> {
    client::post_json("/overtime", req).await
}

pub async fn list_mine(from: &str, to: &str) -> Result<Vec<OvertimeDto>, FrontendError> {
    let q = client::query(&[("from", from), ("to", to)]);
    client::get_json(&format!("/overtime{q}")).await
}

pub async fn cancel(id: OvertimeId) -> Result<OvertimeDto, FrontendError> {
    client::post_empty(&format!("/overtime/{}/cancel", id.0)).await
}

pub async fn leader_queue(group: GroupId) -> Result<Vec<OvertimeDto>, FrontendError> {
    let q = client::query(&[("group", &group.0.to_string())]);
    client::get_json(&format!("/overtime/queue/leader{q}")).await
}

pub async fn hr_queue() -> Result<Vec<OvertimeDto>, FrontendError> {
    client::get_json("/overtime/queue/hr").await
}

pub async fn leader_decision(
    id: OvertimeId,
    req: &DecideOvertimeRequest,
) -> Result<OvertimeDto, FrontendError> {
    client::post_json(&format!("/overtime/{}/leader-decision", id.0), req).await
}

pub async fn hr_decision(
    id: OvertimeId,
    req: &DecideOvertimeRequest,
) -> Result<OvertimeDto, FrontendError> {
    client::post_json(&format!("/overtime/{}/hr-decision", id.0), req).await
}
