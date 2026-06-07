use leptos::prelude::*;

use crate::theme::class;

/// Animated trailing ellipsis that cycles `.` -> `..` -> `...`. Renders only the
/// dots, inheriting `currentColor`, so a caller pairs it with its own label:
/// `view! { "Signing in"<Dots/> }`. The span reserves a fixed width so the
/// growing dots never shift surrounding (e.g. centered) text.
#[component]
pub fn Dots() -> impl IntoView {
    let cls = class(
        "display: inline-block; width: 1.2em; text-align: left; \
         &::after { content: \".\"; animation: portal-ellipsis 1.4s linear infinite; } \
         @keyframes portal-ellipsis { \
           0% { content: \".\"; } \
           33% { content: \"..\"; } \
           66% { content: \"...\"; } \
         }",
    );

    view! { <span class=cls></span> }
}
