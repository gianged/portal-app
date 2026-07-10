use leptos::prelude::*;

use crate::features::leave::components::MyLeave;
use crate::state::title;

#[component]
pub fn LeavePage() -> impl IntoView {
    title::set_page_title("My leave");
    view! { <MyLeave /> }
}
