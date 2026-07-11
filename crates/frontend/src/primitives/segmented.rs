use leptos::ev::MouseEvent;
use leptos::prelude::*;

use crate::theme::{self, color, effect, radius, typography};

/// Pill-style segmented control (e.g. ticket scope: Triage / Assigned / Mine).
/// Wrap a row of [`SegmentedItem`]s; the parent owns the active-segment state.
#[component]
pub fn Segmented(children: Children) -> impl IntoView {
    let cls = theme::class(format!(
        "display: inline-flex; background: {bg}; border: 1px solid {b}; \
         border-radius: {r}; padding: 2px; gap: 2px;",
        bg = color::BG_SUNKEN,
        b = color::BORDER,
        r = radius::MD,
    ));
    view! { <div class=cls>{children()}</div> }
}

#[component]
pub fn SegmentedItem(
    #[prop(into)] active: Signal<bool>,
    #[prop(optional)] on_click: Option<Callback<MouseEvent>>,
    children: Children,
) -> impl IntoView {
    let base = theme::class(format!(
        "padding: 4px 10px; font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         background: transparent; border: none; border-radius: {r}; color: {c}; \
         cursor: pointer; transition: all 120ms ease; \
         &:hover {{ color: {ch}; }}",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        fw = typography::WEIGHT_MEDIUM,
        r = radius::XS,
        c = color::TEXT_MUTED,
        ch = color::TEXT,
    ));
    let active_cls = theme::class(format!(
        "background: {bg} !important; color: {c} !important; box-shadow: {s};",
        bg = color::BG_ELEVATED,
        c = color::TEXT_STRONG,
        s = effect::SHADOW_XS,
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
    view! { <button class=cls on:click=handle>{children()}</button> }
}
