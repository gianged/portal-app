use leptos::prelude::*;

use crate::features::notifications::components::InboxIndex;
use crate::state::title;

#[component]
pub fn InboxPage() -> impl IntoView {
    title::set_page_title("Inbox");
    view! { <InboxIndex /> }
}
