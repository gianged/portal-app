#![allow(dead_code)] // TODO: unused, I will see it

use leptos::prelude::*;

use crate::primitives::stack::Gap;
use crate::theme::class;

#[component]
pub fn Grid(
    #[prop(optional, into)] columns: Option<String>,
    #[prop(optional)] gap: Gap,
    children: Children,
) -> impl IntoView {
    let columns = columns.unwrap_or_else(|| "repeat(auto-fit, minmax(240px, 1fr))".to_string());
    let cls = class(format!(
        "display: grid; grid-template-columns: {c}; gap: {g}; min-width: 0;",
        c = columns,
        g = gap.value(),
    ));
    view! { <div class=cls>{children()}</div> }
}
