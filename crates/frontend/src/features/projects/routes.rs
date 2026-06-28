use leptos::prelude::*;
use leptos_router::hooks;
use uuid::Uuid;

use shared::dto::ids::ProjectId;

use crate::features::home::shell::AuthedPage;
use crate::features::projects::detail::ProjectDetail;
use crate::features::projects::list::ProjectsIndex;

#[component]
pub fn ProjectsPage() -> impl IntoView {
    view! {
        <AuthedPage title="Projects">
            <ProjectsIndex />
        </AuthedPage>
    }
}

#[component]
pub fn ProjectDetailPage() -> impl IntoView {
    let params = hooks::use_params_map();
    let id = Memo::new(move |_| {
        params
            .read()
            .get("id")
            .as_deref()
            .and_then(|s| Uuid::parse_str(s).ok())
            .map(ProjectId)
    });
    view! {
        <AuthedPage title="Project">
            <ProjectDetail id=id />
        </AuthedPage>
    }
}
