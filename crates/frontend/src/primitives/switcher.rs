#![allow(dead_code)] // TODO: unused, I will see it

use leptos::prelude::*;

use crate::primitives::stack::Gap;
use crate::theme::class;

#[component]
pub fn Switcher(
    #[prop(optional)] gap: Gap,
    #[prop(optional, into)] threshold: Option<String>,
    children: Children,
) -> impl IntoView {
    let threshold = threshold.unwrap_or_else(|| "30rem".to_string());
    let cls = class(format!(
        "display: flex; flex-wrap: wrap; gap: {g}; \
         & > * {{ flex-grow: 1; flex-basis: calc(({t} - 100%) * 999); }}",
        g = gap.value(),
        t = threshold,
    ));
    view! { <div class=cls>{children()}</div> }
}
