use leptos::prelude::*;
use leptos_router::hooks;
use uuid::Uuid;

use shared::dto::ids::ProjectId;

use crate::features::projects::detail::ProjectDetail;
use crate::features::projects::list::ProjectsIndex;
use crate::state::title;

#[component]
pub fn ProjectsPage() -> impl IntoView {
    title::set_page_title("Projects");
    view! { <ProjectsIndex /> }
}

#[component]
pub fn ProjectDetailPage() -> impl IntoView {
    title::set_page_title("Project");
    let params = hooks::use_params_map();
    let id = Memo::new(move |_| {
        params
            .read()
            .get("id")
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(ProjectId)
    });
    view! { <ProjectDetail id=id /> }
}
