use leptos::{ev::Event, prelude::*};

use crate::theme::{self, color, effect, radius, space, typography};

/// Multi-line text input (request/ticket descriptions, message composer).
/// Vertically resizable; `on_input` yields the current value on every keystroke.
#[component]
pub fn Textarea(
    #[prop(into)] value: Signal<String>,
    #[prop(optional)] on_input: Option<Callback<String>>,
    #[prop(optional, into)] placeholder: Option<String>,
    #[prop(optional)] rows: Option<u32>,
    #[prop(optional, into)] disabled: Signal<bool>,
) -> impl IntoView {
    let placeholder = placeholder.unwrap_or_default();
    let cls = theme::class(format!(
        "display: block; width: 100%; padding: {p}; background: {bg}; color: {fg}; \
         border: 1px solid {bc}; border-radius: {r}; font-family: {ff}; font-size: {fs}; \
         line-height: 1.5; min-height: 80px; resize: vertical; box-shadow: {s}; \
         transition: border-color 120ms ease, box-shadow 120ms ease; \
         &::placeholder {{ color: {phc}; }} \
         &:hover {{ border-color: {bsc}; }} \
         &:focus {{ outline: none; border-color: {bfc}; box-shadow: {ring}; }} \
         &:disabled {{ background: {bgd}; cursor: not-allowed; opacity: 0.7; }}",
        p = space::D3,
        bg = color::BG_ELEVATED,
        fg = color::TEXT,
        bc = color::BORDER,
        r = radius::MD,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        s = effect::SHADOW_XS,
        phc = color::TEXT_FAINT,
        bsc = color::BORDER_STRONG,
        bfc = color::BORDER_FOCUS,
        ring = effect::RING,
        bgd = color::BG_SUNKEN,
    ));
    let handle = move |ev: Event| {
        if let Some(cb) = on_input {
            cb.run(event_target_value(&ev));
        }
    };
    view! {
        <textarea
            class=cls
            placeholder=placeholder
            rows=rows.unwrap_or(4)
            disabled=move || disabled.get()
            prop:value=move || value.get()
            on:input=handle
        ></textarea>
    }
}
