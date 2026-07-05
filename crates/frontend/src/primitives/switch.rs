#![allow(dead_code)]

use leptos::prelude::*;

use crate::theme::{self, color, radius};

/// A controlled on/off toggle. The parent owns the `on` signal; `on_change`
/// yields the toggled value.
#[component]
pub fn Switch(
    #[prop(into)] on: Signal<bool>,
    #[prop(optional)] on_change: Option<Callback<bool>>,
) -> impl IntoView {
    let base = theme::class(format!(
        "position: relative; width: 32px; height: 18px; border-radius: {pill}; \
         background: {bs}; border: none; padding: 0; cursor: pointer; flex-shrink: 0; \
         transition: background 120ms ease; \
         &::after {{ content: ''; position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; \
            border-radius: 50%; background: #fff; box-shadow: 0 1px 2px rgba(0,0,0,0.2); \
            transition: transform 160ms cubic-bezier(.4,.0,.2,1); }}",
        pill = radius::PILL,
        bs = color::BORDER_STRONG,
    ));
    let on_cls = theme::class(format!(
        "background: {a} !important; &::after {{ transform: translateX(14px); }}",
        a = color::ACCENT,
    ));
    let handle = move |_| {
        if let Some(cb) = on_change {
            cb.run(!on.get());
        }
    };
    let cls = move || {
        if on.get() {
            format!("{base} {on_cls}")
        } else {
            base.clone()
        }
    };
    view! {
        <button
            type="button"
            class=cls
            role="switch"
            aria-checked=move || on.get().to_string()
            on:click=handle
        ></button>
    }
}
