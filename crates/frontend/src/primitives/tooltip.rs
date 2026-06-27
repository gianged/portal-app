#![allow(dead_code)] // TODO: unused

use leptos::prelude::*;

use crate::theme;

#[component]
pub fn Tooltip(#[prop(into)] label: String, children: Children) -> impl IntoView {
    let cls = theme::class("display: inline-flex;");
    view! { <span class=cls title=label>{children()}</span> }
}
