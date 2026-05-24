use leptos::prelude::*;

use crate::theme::{class, color, space};

#[component]
pub fn SidebarLayout(side: AnyView, main: AnyView) -> impl IntoView {
    let wrap = class(format!(
        "display: flex; min-height: 100vh; background: {bg};",
        bg = color::BG,
    ));
    let aside = class(format!(
        "width: {w}; flex-shrink: 0; border-right: 1px solid {b}; background: {bg}; \
         position: sticky; top: 0; height: 100vh; overflow-y: auto;",
        w = space::SIDEBAR_W,
        b = color::BORDER,
        bg = color::BG_SUBTLE,
    ));
    let main_cls = class("flex: 1; min-width: 0; display: flex; flex-direction: column;");

    view! {
        <div class=wrap>
            <aside class=aside>{side}</aside>
            <div class=main_cls>{main}</div>
        </div>
    }
}
