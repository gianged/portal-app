use leptos::prelude::*;

use crate::theme::{class, color, radius, space, typography};

#[component]
pub fn Dialog(
    #[prop(into)] open: Signal<bool>,
    #[prop(optional)] on_close: Option<Callback<()>>,
    children: ChildrenFn,
) -> impl IntoView {
    let backdrop = class(format!(
        "position: fixed; inset: 0; background: rgba(13, 27, 62, 0.45); \
         display: flex; align-items: center; justify-content: center; z-index: 100; \
         padding: {p};",
        p = space::D5,
    ));
    let dialog = class(format!(
        "background: {bg}; border: 1px solid {b}; border-radius: {r}; \
         box-shadow: {s}; width: 100%; max-width: 480px; padding: {p};",
        bg = color::BG_ELEVATED,
        b = color::BORDER,
        r = radius::LG,
        s = typography::SHADOW_LG,
        p = space::D6,
    ));

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
