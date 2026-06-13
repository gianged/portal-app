use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;

use shared::dto::ids::GroupId;

use crate::features::groups::detail::GroupDetail;
use crate::features::groups::list::GroupsIndex;
use crate::features::home::shell::AuthedPage;

#[component]
pub fn GroupsPage() -> impl IntoView {
    view! {
        <AuthedPage title="Groups">
            <GroupsIndex />
        </AuthedPage>
    }
}

#[component]
pub fn GroupDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = Memo::new(move |_| {
        params
            .read()
            .get("id")
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(GroupId)
    });
    view! {
        <AuthedPage title="Group">
            <GroupDetail id=id />
        </AuthedPage>
    }
}
