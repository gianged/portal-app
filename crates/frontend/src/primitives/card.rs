#![allow(dead_code)] // TODO: unused

use leptos::prelude::*;

use crate::theme::{self, color, effect, radius, space};

#[component]
pub fn Card(#[prop(optional, into)] padding: Option<String>, children: Children) -> impl IntoView {
    let padding = padding.unwrap_or_else(|| format!("{} {}", space::D4, space::D5));
    let cls = theme::class(format!(
        "background: {bg}; border: 1px solid {b}; border-radius: {r}; \
         box-shadow: {s}; padding: {p};",
        bg = color::BG_ELEVATED,
        b = color::BORDER,
        r = radius::LG,
        s = effect::SHADOW_XS,
        p = padding,
    ));
    view! { <div class=cls>{children()}</div> }
}

#[component]
pub fn CardHeader(children: Children) -> impl IntoView {
    let cls = theme::class(format!(
        "display: flex; align-items: center; justify-content: space-between; \
         padding-bottom: {p}; border-bottom: 1px solid {b}; margin-bottom: {p};",
        p = space::D3,
        b = color::BORDER,
    ));
    view! { <div class=cls>{children()}</div> }
}

#[component]
pub fn CardBody(children: Children) -> impl IntoView {
    view! { <div>{children()}</div> }
}

#[component]
pub fn CardFooter(children: Children) -> impl IntoView {
    let cls = theme::class(format!(
        "display: flex; align-items: center; justify-content: flex-end; gap: {g}; \
         padding-top: {p}; border-top: 1px solid {b}; margin-top: {p};",
        g = space::D2,
        p = space::D3,
        b = color::BORDER,
    ));
    view! { <div class=cls>{children()}</div> }
}
