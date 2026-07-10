use leptos::prelude::*;
use leptos_router::hooks;
use uuid::Uuid;

use shared::dto::ids::UserId;

use crate::features::users::detail::UserDetail;
use crate::features::users::list::UsersIndex;
use crate::state::title;

#[component]
pub fn UsersPage() -> impl IntoView {
    title::set_page_title("People");
    view! { <UsersIndex /> }
}

#[component]
pub fn UserDetailPage() -> impl IntoView {
    title::set_page_title("Profile");
    let params = hooks::use_params_map();
    let id = Memo::new(move |_| {
        params
            .read()
            .get("id")
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(UserId)
    });
    view! { <UserDetail id=id /> }
}
