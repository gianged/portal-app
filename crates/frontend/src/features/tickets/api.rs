//! IT-ticket HTTP wrappers. Listing is scope-filtered (mine / assigned / triage
//! queue); the lifecycle endpoints each return the updated [`TicketDto`].

use shared::dto::ids::TicketId;
use shared::dto::ticket::{
    AssignTicketRequest, RaiseTicketRequest, TicketDto, TriageTicketRequest,
};

use crate::api::client;
use crate::api::error::FrontendError;

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

pub async fn list(scope: Scope) -> Result<Vec<TicketDto>, FrontendError> {
    let q = client::query(&[("scope", scope.wire())]);
    client::get_json(&format!("/tickets{q}")).await
}

pub async fn get(id: TicketId) -> Result<TicketDto, FrontendError> {
    client::get_json(&format!("/tickets/{}", id.0)).await
}

pub async fn raise(req: &RaiseTicketRequest) -> Result<TicketDto, FrontendError> {
    client::post_json("/tickets", req).await
}

pub async fn triage(id: TicketId, req: &TriageTicketRequest) -> Result<TicketDto, FrontendError> {
    client::post_json(&format!("/tickets/{}/triage", id.0), req).await
}

pub async fn assign(id: TicketId, req: &AssignTicketRequest) -> Result<TicketDto, FrontendError> {
    client::post_json(&format!("/tickets/{}/assign", id.0), req).await
}

pub async fn start(id: TicketId) -> Result<TicketDto, FrontendError> {
    client::post_empty(&format!("/tickets/{}/start", id.0)).await
}

pub async fn resolve(id: TicketId) -> Result<TicketDto, FrontendError> {
    client::post_empty(&format!("/tickets/{}/resolve", id.0)).await
}

pub async fn reject(id: TicketId) -> Result<TicketDto, FrontendError> {
    client::post_empty(&format!("/tickets/{}/reject", id.0)).await
}

pub async fn close(id: TicketId) -> Result<TicketDto, FrontendError> {
    client::post_empty(&format!("/tickets/{}/close", id.0)).await
}

pub async fn reopen(id: TicketId) -> Result<TicketDto, FrontendError> {
    client::post_empty(&format!("/tickets/{}/reopen", id.0)).await
}
