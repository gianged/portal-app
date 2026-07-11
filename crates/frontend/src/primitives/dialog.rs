use leptos::ev::{KeyboardEvent, keydown};
use leptos::prelude::*;

use crate::theme::{self, color, effect, radius, space, typography};

/// Modal dialog. The parent owns the `open` signal; `on_close` fires on backdrop
/// click and on `Escape`. Compose the body from [`DialogHeader`] / [`DialogBody`]
/// / [`DialogFooter`].
#[component]
pub fn Dialog(
    #[prop(into)] open: Signal<bool>,
    #[prop(optional)] on_close: Option<Callback<()>>,
    children: ChildrenFn,
) -> impl IntoView {
    let backdrop = theme::class(format!(
        "position: fixed; inset: 0; background: rgba(13, 27, 62, 0.45); \
         display: flex; align-items: center; justify-content: center; z-index: 100; \
         padding: {p};",
        p = space::D5,
    ));
    let dialog = theme::class(format!(
        "background: {bg}; border: 1px solid {b}; border-radius: {r}; \
         box-shadow: {s}; width: 100%; max-width: 480px; padding: {p}; \
         max-height: calc(100vh - 2 * {p}); overflow-y: auto;",
        bg = color::BG_ELEVATED,
        b = color::BORDER,
        r = radius::LG,
        s = effect::SHADOW_LG,
        p = space::D6,
    ));

    // Close on Escape; reads `open` untracked since the handler is not a reactive context.
    let handle = window_event_listener(keydown, move |ev: KeyboardEvent| {
        if open.get_untracked()
            && ev.key() == "Escape"
            && let Some(cb) = on_close
        {
            cb.run(());
        }
    });
    on_cleanup(move || handle.remove());

    view! {
        <Show when=move || open.get() fallback=|| ()>
            <div
                class=backdrop.clone()
                on:click=move |_| if let Some(cb) = on_close { cb.run(()) }
            >
                <div class=dialog.clone() on:click=|ev| ev.stop_propagation()>
                    {children()}
                </div>
            </div>
        </Show>
    }
}

/// Title + optional subtitle block for a [`Dialog`].
#[component]
pub fn DialogHeader(
    #[prop(into)] title: String,
    #[prop(optional, into)] subtitle: Option<String>,
) -> impl IntoView {
    let title_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; \
         margin: 0; letter-spacing: -0.015em;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H3,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let sub_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; margin: {mt} 0 0;",
        mt = space::D1,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));
    let wrap = theme::class(format!("margin-bottom: {mb};", mb = space::D4));
    view! {
        <div class=wrap>
            <h3 class=title_cls>{title}</h3>
            {subtitle.map(|s| view! { <p class=sub_cls>{s}</p> })}
        </div>
    }
}

/// Content region of a [`Dialog`].
#[component]
pub fn DialogBody(children: Children) -> impl IntoView {
    view! { <div>{children()}</div> }
}

/// Right-aligned action row for a [`Dialog`], separated by a top border.
#[component]
pub fn DialogFooter(children: Children) -> impl IntoView {
    let cls = theme::class(format!(
        "display: flex; align-items: center; justify-content: flex-end; gap: {g}; \
         padding-top: {p}; border-top: 1px solid {b}; margin-top: {p};",
        g = space::D2,
        p = space::D4,
        b = color::BORDER,
    ));
    view! { <div class=cls>{children()}</div> }
}
