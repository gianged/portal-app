#![allow(dead_code)] // TODO: unused

use leptos::prelude::*;

use crate::primitives::icon::{Icon, IconName};
use crate::theme::{self, color, typography};

/// A controlled checkbox. The parent owns the `checked` signal; `on_change`
/// yields the toggled value. An optional inline `label` makes the whole row
/// clickable.
#[component]
pub fn Checkbox(
    #[prop(into)] checked: Signal<bool>,
    #[prop(optional)] on_change: Option<Callback<bool>>,
    #[prop(optional, into)] label: Option<String>,
) -> impl IntoView {
    let wrap = theme::class(format!(
        "display: inline-flex; align-items: center; gap: 8px; cursor: pointer; \
         font-family: {ff}; font-size: {fs}; color: {c}; user-select: none;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    ));
    let base = theme::class(format!(
        "width: 16px; height: 16px; border: 1px solid {bs}; background: {bg}; \
         border-radius: 4px; display: inline-flex; align-items: center; justify-content: center; \
         flex-shrink: 0; transition: background 120ms ease, border-color 120ms ease; \
         &:hover {{ border-color: {a}; }}",
        bs = color::BORDER_STRONG,
        bg = color::BG_ELEVATED,
        a = color::ACCENT,
    ));
    let checked_box = theme::class(format!(
        "background: {a} !important; border-color: {a} !important; color: {fg};",
        a = color::ACCENT,
        fg = color::TEXT_ON_ACCENT,
    ));

    let on_click = move |_| {
        if let Some(cb) = on_change {
            cb.run(!checked.get());
        }
    };
    let box_cls = move || {
        if checked.get() {
            format!("{base} {checked_box}")
        } else {
            base.clone()
        }
    };

    view! {
        <label class=wrap on:click=on_click>
            <span class=box_cls>
                <Show when=move || checked.get() fallback=|| ()>
                    <Icon name=IconName::Check size=11 />
                </Show>
            </span>
            {label.map(|l| view! { <span>{l}</span> })}
        </label>
    }
}
