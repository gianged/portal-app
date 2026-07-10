use leptos::prelude::*;
use leptos_router::hooks;
use uuid::Uuid;

use shared::dto::ids::TicketId;

use crate::features::tickets::detail::TicketDetail;
use crate::features::tickets::list::TicketsIndex;
use crate::state::title;

#[component]
pub fn TicketsPage() -> impl IntoView {
    title::set_page_title("IT tickets");
    view! { <TicketsIndex /> }
}

#[component]
pub fn TicketDetailPage() -> impl IntoView {
    title::set_page_title("Ticket");
    let params = hooks::use_params_map();
    let id = Memo::new(move |_| {
        params
            .read()
            .get("id")
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(TicketId)
    });
    view! { <TicketDetail id=id /> }
}
