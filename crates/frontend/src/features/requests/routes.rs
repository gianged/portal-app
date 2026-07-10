use leptos::prelude::*;
use leptos_router::hooks;
use uuid::Uuid;

use shared::dto::ids::RequestId;

use crate::features::requests::detail::RequestDetail;
use crate::features::requests::list::RequestsIndex;
use crate::state::title;

#[component]
pub fn RequestsPage() -> impl IntoView {
    title::set_page_title("Requests");
    view! { <RequestsIndex /> }
}

#[component]
pub fn RequestDetailPage() -> impl IntoView {
    title::set_page_title("Request");
    let params = hooks::use_params_map();
    let id = Memo::new(move |_| {
        params
            .read()
            .get("id")
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(RequestId)
    });
    view! { <RequestDetail id=id /> }
}
