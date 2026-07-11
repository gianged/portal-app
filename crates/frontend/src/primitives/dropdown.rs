use leptos::ev::{KeyboardEvent, MouseEvent, click, keydown};
use leptos::html::Div;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::theme::{self, color, effect, radius, space};

/// Click-to-toggle popover. The `trigger` is rendered inline; clicking it shows
/// the menu (this component's children) anchored to the trigger's bottom-right.
/// Outside click, Escape, and a click on a menu item all dismiss it.
#[component]
pub fn Dropdown(trigger: AnyView, children: ChildrenFn) -> impl IntoView {
    let open = RwSignal::new(false);
    let wrap_ref = NodeRef::<Div>::new();

    let wrap = theme::class("position: relative; display: inline-block;");
    let menu_cls = theme::class(format!(
        "position: absolute; top: calc(100% + {g}); right: 0; min-width: 200px; \
         background: {bg}; border: 1px solid {b}; border-radius: {r}; \
         box-shadow: {s}; padding: {p}; z-index: 50;",
        g = space::D1,
        bg = color::BG_ELEVATED,
        b = color::BORDER,
        r = radius::MD,
        s = effect::SHADOW_MD,
        p = space::D2,
    ));

    // Dismiss on outside click or Escape, mirroring Dialog.
    let click_handle = window_event_listener(click, move |ev: MouseEvent| {
        if !open.get_untracked() {
            return;
        }
        let inside = wrap_ref.get_untracked().is_some_and(|el| {
            ev.target()
                .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
                .is_some_and(|target| el.contains(Some(&target)))
        });
        if !inside {
            open.set(false);
        }
    });
    let key_handle = window_event_listener(keydown, move |ev: KeyboardEvent| {
        if open.get_untracked() && ev.key() == "Escape" {
            open.set(false);
        }
    });
    on_cleanup(move || {
        click_handle.remove();
        key_handle.remove();
    });

    view! {
        <div node_ref=wrap_ref class=wrap>
            <div on:click=move |_| open.update(|v| *v = !*v)>{trigger}</div>
            <Show when=move || open.get() fallback=|| ()>
                <div class=menu_cls.clone() on:click=move |_| open.set(false)>{children()}</div>
            </Show>
        </div>
    }
}
