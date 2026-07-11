#![allow(dead_code)] // TODO: unused

use leptos::ev::Event;
use leptos::prelude::*;

use crate::primitives::icon::{Icon, IconName};
use crate::theme::{self, color, effect, radius, space, typography};

/// A controlled checkbox backed by a visually hidden native `<input>`, so focus,
/// keyboard toggling, and ARIA state come for free. The parent owns the `checked`
/// signal; `on_change` yields the toggled value. An optional inline `label` makes
/// the whole row clickable.
#[component]
pub fn Checkbox(
    #[prop(into)] checked: Signal<bool>,
    #[prop(optional)] on_change: Option<Callback<bool>>,
    #[prop(optional, into)] label: Option<String>,
) -> impl IntoView {
    let wrap = theme::class(format!(
        "position: relative; display: inline-flex; align-items: center; gap: {g}; \
         cursor: pointer; font-family: {ff}; font-size: {fs}; color: {c}; user-select: none;",
        g = space::D2,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    ));
    let input_cls = theme::class(format!(
        "position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; border: 0; \
         clip: rect(0 0 0 0); overflow: hidden; \
         &:focus-visible + span {{ box-shadow: {ring}; }}",
        ring = effect::RING,
    ));
    let base = theme::class(format!(
        "width: 16px; height: 16px; border: 1px solid {bs}; background: {bg}; \
         border-radius: {r}; display: inline-flex; align-items: center; justify-content: center; \
         flex-shrink: 0; transition: background 120ms ease, border-color 120ms ease; \
         &:hover {{ border-color: {a}; }}",
        bs = color::BORDER_STRONG,
        bg = color::BG_ELEVATED,
        r = radius::XS,
        a = color::ACCENT,
    ));
    let checked_box = theme::class(format!(
        "background: {a} !important; border-color: {a} !important; color: {fg};",
        a = color::ACCENT,
        fg = color::TEXT_ON_ACCENT,
    ));

    let handle_change = move |ev: Event| {
        if let Some(cb) = on_change {
            cb.run(event_target_checked(&ev));
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
        <label class=wrap>
            <input
                type="checkbox"
                class=input_cls
                prop:checked=move || checked.get()
                on:change=handle_change
            />
            <span class=box_cls aria-hidden="true">
                <Show when=move || checked.get() fallback=|| ()>
                    <Icon name=IconName::Check size=11 />
                </Show>
            </span>
            {label.map(|l| view! { <span>{l}</span> })}
        </label>
    }
}
