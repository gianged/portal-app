use leptos::prelude::*;

use crate::theme::{self, color, radius, space, typography};

/// A centered "load more / load older" button with a busy state. Used to page
/// through chat history (cursor-based) and other append-style lists.
#[component]
pub fn LoadMore(
    #[prop(optional)] on_click: Option<Callback<()>>,
    #[prop(optional)] loading: Signal<bool>,
    #[prop(optional, into)] label: Option<String>,
) -> impl IntoView {
    let label = label.unwrap_or_else(|| "Load more".to_owned());
    let wrap = theme::class(format!(
        "display: flex; justify-content: center; padding: {p};",
        p = space::D3,
    ));
    let btn = theme::class(format!(
        "background: transparent; border: 1px solid {b}; border-radius: {r}; \
         padding: 6px {px}; font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         color: {c}; cursor: pointer; transition: background 120ms ease; \
         &:hover:not(:disabled) {{ background: {bh}; }} \
         &:disabled {{ opacity: 0.6; cursor: default; }}",
        b = color::BORDER,
        r = radius::MD,
        px = space::D4,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        fw = typography::WEIGHT_MEDIUM,
        c = color::TEXT_MUTED,
        bh = color::BG_HOVER,
    ));
    let handle = move |_| {
        if let Some(cb) = on_click {
            cb.run(());
        }
    };
    view! {
        <div class=wrap>
            <button
                class=btn
                disabled=move || loading.get()
                on:click=handle
            >
                {move || if loading.get() { "Loading…".to_owned() } else { label.clone() }}
            </button>
        </div>
    }
}
