use leptos::prelude::*;

use crate::features::auth::components::RequireAuth;
use crate::features::home::components::{SidebarNav, Topbar};
use crate::primitives::sidebar::SidebarLayout;
use crate::theme::{class, space};

/// Signed-in page: the auth guard wrapping [`AppShell`], so the guard and frame aren't repeated per page.
#[component]
pub fn AuthedPage(#[prop(into)] title: String, children: ChildrenFn) -> impl IntoView {
    // RequireAuth re-renders its children, so stash the page body in a StoredValue to call it each render.
    let children = StoredValue::new(children);
    view! {
        <RequireAuth>
            <AppShell title=title.clone()>
                {move || children.with_value(|c| c())}
            </AppShell>
        </RequireAuth>
    }
}

/// Authenticated app frame: fixed sidebar, sticky topbar, and a centered padded content column.
#[component]
pub fn AppShell(#[prop(into)] title: String, children: Children) -> impl IntoView {
    let main_cls = class(format!(
        "padding: {p1} {p2} {p3}; max-width: {mw}; width: 100%; margin: 0 auto;",
        p1 = space::D6,
        p2 = space::D7,
        p3 = space::D8,
        mw = space::CONTENT_MAX_W,
    ));

    let side = view! { <SidebarNav /> }.into_any();
    let main = view! {
        <Topbar title=title />
        <main class=main_cls>{children()}</main>
    }
    .into_any();

    view! { <SidebarLayout side=side main=main /> }
}
