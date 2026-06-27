#![allow(dead_code)] // TODO: unused

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::theme::{self, color, radius, space, typography};

/// Underline tab bar. Wrap a row of [`Tab`]s; each `Tab` takes a reactive `active`
/// signal and an `on_click`, so the parent owns the selected-tab state.
#[component]
pub fn Tabs(children: Children) -> impl IntoView {
    let cls = theme::class(format!(
        "display: flex; align-items: center; gap: 2px; border-bottom: 1px solid {b};",
        b = color::BORDER,
    ));
    view! { <div class=cls role="tablist">{children()}</div> }
}

#[component]
pub fn Tab(
    #[prop(into)] active: Signal<bool>,
    #[prop(optional)] count: Option<u32>,
    #[prop(optional)] on_click: Option<Callback<MouseEvent>>,
    children: Children,
) -> impl IntoView {
    let base = theme::class(format!(
        "display: inline-flex; align-items: center; gap: 6px; padding: {py} {px}; \
         font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; \
         background: transparent; border: none; border-bottom: 2px solid transparent; \
         margin-bottom: -1px; cursor: pointer; \
         transition: color 120ms ease, border-color 120ms ease; \
         &:hover {{ color: {ch}; }}",
        py = space::D3,
        px = space::D4,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_MEDIUM,
        c = color::TEXT_MUTED,
        ch = color::TEXT,
    ));
    let active_cls = theme::class(format!(
        "color: {c} !important; border-bottom-color: {c} !important;",
        c = color::ACCENT,
    ));
    let count_cls = theme::class(format!(
        "font-size: 11px; padding: 1px 6px; border-radius: {r}; \
         background: {bg}; color: {c}; font-weight: {fw};",
        r = radius::PILL,
        bg = color::BG_SUNKEN,
        c = color::TEXT_MUTED,
        fw = typography::WEIGHT_MEDIUM,
    ));
    let handle = move |ev: MouseEvent| {
        if let Some(cb) = on_click {
            cb.run(ev);
        }
    };
    let cls = move || {
        if active.get() {
            format!("{base} {active_cls}")
        } else {
            base.clone()
        }
    };
    view! {
        <button class=cls role="tab" on:click=handle>
            {children()}
            {count.map(|n| view! { <span class=count_cls.clone()>{n}</span> })}
        </button>
    }
}
