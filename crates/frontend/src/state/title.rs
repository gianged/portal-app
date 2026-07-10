//! Current page title shown in the shared topbar. Provided once by the authed
//! layout so routed pages can retitle the persistent shell.

use leptos::prelude::*;

#[derive(Clone, Copy)]
pub struct PageTitleState {
    pub title: RwSignal<String>,
}

impl Default for PageTitleState {
    fn default() -> Self {
        Self::new()
    }
}

impl PageTitleState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            title: RwSignal::new(String::new()),
        }
    }
}

/// Sets the shared topbar title; call once at the top of a routed page component.
pub fn set_page_title(title: &str) {
    let state = use_context::<PageTitleState>().expect("PageTitleState context");
    state.title.set(title.to_owned());
}
