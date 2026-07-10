use leptos::prelude::*;
use leptos_router::hooks;
use uuid::Uuid;

use shared::dto::ids::GroupId;

use crate::features::groups::detail::GroupDetail;
use crate::features::groups::list::GroupsIndex;
use crate::state::title;

#[component]
pub fn GroupsPage() -> impl IntoView {
    title::set_page_title("Groups");
    view! { <GroupsIndex /> }
}

#[component]
pub fn GroupDetailPage() -> impl IntoView {
    title::set_page_title("Group");
    let params = hooks::use_params_map();
    let id = Memo::new(move |_| {
        params
            .read()
            .get("id")
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(GroupId)
    });
    view! { <GroupDetail id=id /> }
}
