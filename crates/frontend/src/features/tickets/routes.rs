use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;

use shared::dto::ids::TicketId;

use crate::features::home::shell::AuthedPage;
use crate::features::tickets::components::{TicketDetail, TicketsIndex};

#[component]
pub fn TicketsPage() -> impl IntoView {
    view! {
        <AuthedPage title="IT tickets">
            <TicketsIndex />
        </AuthedPage>
    }
}

#[component]
pub fn TicketDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = Memo::new(move |_| {
        params
            .read()
            .get("id")
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(TicketId)
    });
    view! {
        <AuthedPage title="Ticket">
            <TicketDetail id=id />
        </AuthedPage>
    }
}
