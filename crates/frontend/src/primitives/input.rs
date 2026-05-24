use leptos::ev::Event;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

use crate::theme::{class, color, radius, space, typography};

#[component]
pub fn Input(
    #[prop(into)] value: Signal<String>,
    #[prop(optional)] on_input: Option<Callback<String>>,
    #[prop(optional, into)] placeholder: Option<String>,
    #[prop(optional, into)] type_: Option<String>,
    #[prop(optional)] disabled: bool,
    #[prop(optional, into)] autocomplete: Option<String>,
) -> impl IntoView {
    let placeholder = placeholder.unwrap_or_default();
    let type_ = type_.unwrap_or_else(|| "text".to_string());
    let autocomplete = autocomplete.unwrap_or_else(|| "off".to_string());

    let cls = class(format!(
        "display: block; width: 100%; height: {h}; padding: 0 {px}; \
         background: {bg}; color: {fg}; \
         border: 1px solid {bc}; border-radius: {r}; \
         font-family: {ff}; font-size: {fs}; line-height: 1.4; \
         transition: border-color 120ms ease, box-shadow 120ms ease; \
         &::placeholder {{ color: {phc}; }} \
         &:focus {{ outline: none; border-color: {bfc}; box-shadow: {ring}; }} \
         &:disabled {{ background: {bgd}; cursor: not-allowed; opacity: 0.7; }}",
        h = space::INPUT_H,
        px = space::D3,
        bg = color::BG_ELEVATED,
        fg = color::TEXT,
        bc = color::BORDER,
        r = radius::MD,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_BODY,
        phc = color::TEXT_FAINT,
        bfc = color::BORDER_FOCUS,
        ring = typography::RING,
        bgd = color::BG_SUNKEN,
    ));

    let handle_input = move |ev: Event| {
        if let Some(cb) = on_input {
            let target = ev.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok());
            if let Some(el) = target {
                cb.run(el.value());
            }
        }
    };

    view! {
        <input
            class=cls
            type=type_
            placeholder=placeholder
            autocomplete=autocomplete
            disabled=disabled
            prop:value=move || value.get()
            on:input=handle_input
        />
    }
}

#[component]
pub fn FieldLabel(
    #[prop(into)] for_id: String,
    children: Children,
) -> impl IntoView {
    let cls = class(format!(
        "display: block; font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         color: {c}; margin-bottom: {mb};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_MEDIUM,
        c = color::TEXT_STRONG,
        mb = space::D1,
    ));
    view! { <label class=cls for=for_id>{children()}</label> }
}

#[component]
pub fn FieldError(#[prop(into)] message: String) -> impl IntoView {
    let cls = class(format!(
        "color: {c}; font-family: {ff}; font-size: {fs}; margin-top: {mt};",
        c = color::DANGER,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        mt = space::D1,
    ));
    view! { <div class=cls>{message}</div> }
}
