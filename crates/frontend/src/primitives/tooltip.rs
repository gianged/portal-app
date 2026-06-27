#![allow(dead_code)] // TODO: unused

use leptos::prelude::*;

use crate::theme::class;

#[component]
pub fn Tooltip(#[prop(into)] label: String, children: Children) -> impl IntoView {
    let cls = class("display: inline-flex;");
    view! { <span class=cls title=label>{children()}</span> }
}
