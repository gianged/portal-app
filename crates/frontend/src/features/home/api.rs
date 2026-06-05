//! Dashboard data. There is no bespoke "summary" endpoint — the dashboard is
//! assembled client-side from the same list endpoints the feature pages use.

use shared::dto::{
    chat::ChannelSummaryDto, group::GroupDto, request::RequestDto, ticket::TicketDto,
};

use crate::api::client;
use crate::api::error::FrontendError;

/// Work requests assigned to the caller.
pub async fn my_requests() -> Result<Vec<RequestDto>, FrontendError> {
    client::get_json("/requests?mine=true").await
}

/// IT tickets the caller raised.
pub async fn my_tickets() -> Result<Vec<TicketDto>, FrontendError> {
    client::get_json("/tickets?scope=mine").await
}

/// The caller's chat channels (group / general / direct), for the sidebar panel.
pub async fn channels() -> Result<Vec<ChannelSummaryDto>, FrontendError> {
    client::get_json("/chat/channels").await
}

/// All groups in the org, with member counts.
pub async fn groups() -> Result<Vec<GroupDto>, FrontendError> {
    client::get_json("/groups").await
}
