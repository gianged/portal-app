//! Flexible-hours HTTP wrappers. `work_date` is `"YYYY-MM-DD"`; the leader queue
//! and decision are leader-gated server-side.

use shared::dto::flex_hours::{
    DecideFlexRequest, FlexHoursDto, FlexMonthDeltaDto, RequestFlexRequest,
};
use shared::dto::ids::{FlexHoursId, GroupId};

use crate::api::client;
use crate::api::error::FrontendError;

pub async fn create(req: &RequestFlexRequest) -> Result<FlexHoursDto, FrontendError> {
    client::post_json("/flex-hours", req).await
}

pub async fn list_mine(from: &str, to: &str) -> Result<Vec<FlexHoursDto>, FrontendError> {
    let q = client::query(&[("from", from), ("to", to)]);
    client::get_json(&format!("/flex-hours{q}")).await
}

pub async fn month_delta(year: i32, month: u32) -> Result<FlexMonthDeltaDto, FrontendError> {
    let q = client::query(&[("year", &year.to_string()), ("month", &month.to_string())]);
    client::get_json(&format!("/flex-hours/month-delta{q}")).await
}

pub async fn cancel(id: FlexHoursId) -> Result<FlexHoursDto, FrontendError> {
    client::post_empty(&format!("/flex-hours/{}/cancel", id.0)).await
}

pub async fn leader_queue(group: GroupId) -> Result<Vec<FlexHoursDto>, FrontendError> {
    let q = client::query(&[("group", &group.0.to_string())]);
    client::get_json(&format!("/flex-hours/queue/leader{q}")).await
}

pub async fn decision(
    id: FlexHoursId,
    req: &DecideFlexRequest,
) -> Result<FlexHoursDto, FrontendError> {
    client::post_json(&format!("/flex-hours/{}/decision", id.0), req).await
}
