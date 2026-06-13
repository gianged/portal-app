use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use uuid::Uuid;

use shared::dto::ids::UserId;

use crate::features::home::shell::AuthedPage;
use crate::features::users::detail::UserDetail;
use crate::features::users::list::UsersIndex;

#[component]
pub fn UsersPage() -> impl IntoView {
    view! {
        <AuthedPage title="People">
            <UsersIndex />
        </AuthedPage>
    }
}

#[component]
pub fn UserDetailPage() -> impl IntoView {
    let params = use_params_map();
    let id = Memo::new(move |_| {
        params
            .read()
            .get("id")
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(UserId)
    });
    view! {
        <AuthedPage title="Profile">
            <UserDetail id=id />
        </AuthedPage>
    }
}
