//! Day-off (leave) HTTP wrappers. Dates are `"YYYY-MM-DD"`; queues and decisions
//! are leader/HR-gated server-side.

use shared::dto::day_off::{CreateDayOffRequest, DayOffDto, DecideDayOffRequest};
use shared::dto::ids::{DayOffId, GroupId};

use crate::api::client;
use crate::api::error::FrontendError;

pub async fn create(req: &CreateDayOffRequest) -> Result<DayOffDto, FrontendError> {
    client::post_json("/dayoff", req).await
}

pub async fn list_mine(from: &str, to: &str) -> Result<Vec<DayOffDto>, FrontendError> {
    let q = client::query(&[("from", from), ("to", to)]);
    client::get_json(&format!("/dayoff{q}")).await
}

pub async fn cancel(id: DayOffId) -> Result<DayOffDto, FrontendError> {
    client::post_empty(&format!("/dayoff/{}/cancel", id.0)).await
}

pub async fn leader_queue(group: GroupId) -> Result<Vec<DayOffDto>, FrontendError> {
    let q = client::query(&[("group", &group.0.to_string())]);
    client::get_json(&format!("/dayoff/queue/leader{q}")).await
}

pub async fn hr_queue() -> Result<Vec<DayOffDto>, FrontendError> {
    client::get_json("/dayoff/queue/hr").await
}

pub async fn leader_decision(
    id: DayOffId,
    req: &DecideDayOffRequest,
) -> Result<DayOffDto, FrontendError> {
    client::post_json(&format!("/dayoff/{}/leader-decision", id.0), req).await
}

pub async fn hr_decision(
    id: DayOffId,
    req: &DecideDayOffRequest,
) -> Result<DayOffDto, FrontendError> {
    client::post_json(&format!("/dayoff/{}/hr-decision", id.0), req).await
}
