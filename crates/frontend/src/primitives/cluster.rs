use leptos::prelude::*;

use crate::primitives::stack::Gap;
use crate::theme;

#[component]
pub fn Cluster(
    #[prop(optional)] gap: Gap,
    #[prop(optional, into)] align: Option<String>,
    #[prop(optional, into)] justify: Option<String>,
    children: Children,
) -> impl IntoView {
    let align = align.unwrap_or_else(|| "center".to_string());
    let justify = justify.unwrap_or_else(|| "flex-start".to_string());
    let cls = theme::class(format!(
        "display: flex; flex-wrap: wrap; gap: {g}; align-items: {a}; justify-content: {j};",
        g = gap.value(),
        a = align,
        j = justify,
    ));
    view! { <div class=cls>{children()}</div> }
}
