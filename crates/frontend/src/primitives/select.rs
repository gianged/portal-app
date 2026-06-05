use leptos::ev::Event;
use leptos::prelude::*;

use crate::theme::{class, color, radius, space, typography};

/// Native `<select>` styled to match the design system (custom chevron, focus
/// ring). Pass `<option>` elements as children; `on_change` yields the new value.
#[component]
pub fn Select(
    #[prop(into)] value: Signal<String>,
    #[prop(optional)] on_change: Option<Callback<String>>,
    #[prop(optional)] disabled: bool,
    children: Children,
) -> impl IntoView {
    let cls = class(format!(
        "display: block; width: 100%; height: {h}; padding: 0 30px 0 {px}; \
         background-color: {bg}; color: {fg}; border: 1px solid {bc}; border-radius: {r}; \
         font-family: {ff}; font-size: {fs}; box-shadow: {s}; appearance: none; \
         background-image: url(\"data:image/svg+xml,%3Csvg viewBox='0 0 12 12' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M3 4.5l3 3 3-3' stroke='%2351607a' stroke-width='1.5' fill='none' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E\"); \
         background-repeat: no-repeat; background-position: right 10px center; background-size: 12px; \
         transition: border-color 120ms ease, box-shadow 120ms ease; \
         &:hover {{ border-color: {bsc}; }} \
         &:focus {{ outline: none; border-color: {bfc}; box-shadow: {ring}; }} \
         &:disabled {{ background-color: {bgd}; cursor: not-allowed; opacity: 0.7; }}",
        h = space::INPUT_H,
        px = space::D3,
        bg = color::BG_ELEVATED,
        fg = color::TEXT,
        bc = color::BORDER,
        r = radius::MD,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        s = typography::SHADOW_XS,
        bsc = color::BORDER_STRONG,
        bfc = color::BORDER_FOCUS,
        ring = typography::RING,
        bgd = color::BG_SUNKEN,
    ));
    let handle = move |ev: Event| {
        if let Some(cb) = on_change {
            cb.run(event_target_value(&ev));
        }
    };
    view! {
        <select class=cls disabled=disabled prop:value=move || value.get() on:change=handle>
            {children()}
        </select>
    }
}
