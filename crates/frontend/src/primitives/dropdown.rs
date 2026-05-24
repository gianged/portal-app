use leptos::prelude::*;

use crate::theme::{class, color, radius, space, typography};

#[component]
pub fn Dropdown(trigger: ChildrenFn, menu: ChildrenFn) -> impl IntoView {
    let open = RwSignal::new(false);

    let wrap = class("position: relative; display: inline-block;");
    let menu_cls = class(format!(
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
            <div on:click=move |_| open.update(|v| *v = !*v)>{trigger()}</div>
            <Show when=move || open.get() fallback=|| ()>
                <div class=menu_cls.clone()>{menu()}</div>
            </Show>
        </div>
    }
}
