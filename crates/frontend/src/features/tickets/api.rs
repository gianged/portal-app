//! IT-ticket HTTP wrappers; listing is scope-filtered (mine / assigned / triage queue), and the lifecycle endpoints each return the updated [`TicketDto`].

use shared::dto::ids::TicketId;
use shared::dto::ticket::{
    AssignTicketRequest, RaiseTicketRequest, TicketDto, TriageTicketRequest,
};
use web_sys::js_sys;

use crate::api::client;
use crate::api::error::FrontendError;

/// Which tickets to list: the caller's own, those assigned to them, or the triage queue.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Mine,
    Assigned,
    Triage,
}

impl Scope {
    fn wire(self) -> &'static str {
        match self {
            Self::Mine => "mine",
            Self::Assigned => "assigned",
            Self::Triage => "triage",
        }
    }
}

/// Tickets in the given scope (`GET /tickets?scope=…`); `q` filters by title substring.
pub async fn list(scope: Scope, q: Option<String>) -> Result<Vec<TicketDto>, FrontendError> {
    let mut pairs: Vec<(&str, &str)> = vec![("scope", scope.wire())];
    let encoded = q.map(|term| String::from(js_sys::encode_uri_component(&term)));
    if let Some(encoded) = &encoded {
        pairs.push(("q", encoded));
    }
    let query = client::query(&pairs);
    client::get_json(&format!("/tickets{query}")).await
}

/// One ticket by id.
pub async fn get(id: TicketId) -> Result<TicketDto, FrontendError> {
    client::get_json(&format!("/tickets/{}", id.0)).await
}

/// Raise a new ticket.
pub async fn raise(req: &RaiseTicketRequest) -> Result<TicketDto, FrontendError> {
    client::post_json("/tickets", req).await
}

/// Triage an open ticket by setting its priority.
pub async fn triage(id: TicketId, req: &TriageTicketRequest) -> Result<TicketDto, FrontendError> {
    client::post_json(&format!("/tickets/{}/triage", id.0), req).await
}

/// Assign a triaged ticket to an IT staffer.
pub async fn assign(id: TicketId, req: &AssignTicketRequest) -> Result<TicketDto, FrontendError> {
    client::post_json(&format!("/tickets/{}/assign", id.0), req).await
}

/// Start work on an assigned ticket.
pub async fn start(id: TicketId) -> Result<TicketDto, FrontendError> {
    client::post_empty(&format!("/tickets/{}/start", id.0)).await
}

/// Mark an in-progress ticket resolved.
pub async fn resolve(id: TicketId) -> Result<TicketDto, FrontendError> {
    client::post_empty(&format!("/tickets/{}/resolve", id.0)).await
}

/// Reject a resolution, sending the ticket back to in progress.
pub async fn reject(id: TicketId) -> Result<TicketDto, FrontendError> {
    client::post_empty(&format!("/tickets/{}/reject", id.0)).await
}

/// Close a resolved ticket.
pub async fn close(id: TicketId) -> Result<TicketDto, FrontendError> {
    client::post_empty(&format!("/tickets/{}/close", id.0)).await
}

/// Reopen a closed ticket (within the 7-day window).
pub async fn reopen(id: TicketId) -> Result<TicketDto, FrontendError> {
    client::post_empty(&format!("/tickets/{}/reopen", id.0)).await
}
