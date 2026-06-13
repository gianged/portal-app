use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;

use shared::dto::ids::RequestId;

use crate::features::home::shell::AuthedPage;
use crate::features::requests::detail::RequestDetail;
use crate::features::requests::list::RequestsIndex;

#[component]
pub fn RequestsPage() -> impl IntoView {
    view! {
        <AuthedPage title="Requests">
            <RequestsIndex />
        </AuthedPage>
    }
}

#[component]
pub fn RequestDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = Memo::new(move |_| {
        params
            .read()
            .get("id")
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(RequestId)
    });
    view! {
        <AuthedPage title="Request">
            <RequestDetail id=id />
        </AuthedPage>
    }
}
