use leptos::prelude::*;

use crate::theme::{class, space::D6};

#[component]
pub fn Center(
    #[prop(optional, into)] min_height: Option<String>,
    children: Children,
) -> impl IntoView {
    let min_height = min_height.unwrap_or_else(|| "100vh".to_string());
    let cls = class(format!(
        "display: flex; align-items: center; justify-content: center; min-height: {min_height}; padding: {D6};",
    ));
    view! { <div class=cls>{children()}</div> }
}
