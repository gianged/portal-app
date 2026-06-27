use leptos::prelude::*;

use crate::theme::{self, color, radius, space, typography};

/// Click-to-toggle popover. The `trigger` is rendered inline; clicking it shows
/// the menu (this component's children) anchored to the trigger's bottom-right.
#[component]
pub fn Dropdown(trigger: AnyView, children: ChildrenFn) -> impl IntoView {
    let open = RwSignal::new(false);

    let wrap = theme::class("position: relative; display: inline-block;");
    let menu_cls = theme::class(format!(
        "position: absolute; top: calc(100% + {g}); right: 0; min-width: 200px; \
         background: {bg}; border: 1px solid {b}; border-radius: {r}; \
         box-shadow: {s}; padding: {p}; z-index: 50;",
        g = space::D1,
        bg = color::BG_ELEVATED,
        b = color::BORDER,
        r = radius::MD,
        s = typography::SHADOW_MD,
        p = space::D2,
    ));

    view! {
        <div class=wrap>
            <div on:click=move |_| open.update(|v| *v = !*v)>{trigger}</div>
            <Show when=move || open.get() fallback=|| ()>
                <div class=menu_cls.clone()>{children()}</div>
            </Show>
        </div>
    }
}
