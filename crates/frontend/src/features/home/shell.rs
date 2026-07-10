use leptos::prelude::*;
use leptos_router::components::Outlet;

use crate::features::auth::components::RequireAuth;
use crate::features::home::components::{SidebarNav, Topbar};
use crate::primitives::sidebar::SidebarLayout;
use crate::state::chat::ChatUiState;
use crate::state::title::PageTitleState;
use crate::theme::{self, space};

/// Persistent signed-in layout: mounted once by the router, so the sidebar and
/// topbar (and their DOM state, e.g. sidebar scroll) survive navigation.
#[component]
pub fn AuthedLayout() -> impl IntoView {
    provide_context(PageTitleState::new());
    provide_context(ChatUiState::new());
    view! {
        <RequireAuth>
            <AppShell>
                <Outlet />
            </AppShell>
        </RequireAuth>
    }
}

/// Authenticated app frame: fixed sidebar, sticky topbar, and a centered padded content column.
#[component]
pub fn AppShell(children: Children) -> impl IntoView {
    let main_cls = theme::class(format!(
        "padding: {p1} {p2} {p3}; max-width: {mw}; width: 100%; margin: 0 auto;",
        p1 = space::D6,
        p2 = space::D7,
        p3 = space::D8,
        mw = space::CONTENT_MAX_W,
    ));

    let side = view! { <SidebarNav /> }.into_any();
    let main = view! {
        <Topbar />
        <main class=main_cls>{children()}</main>
    }
    .into_any();

    view! { <SidebarLayout side=side main=main /> }
}
