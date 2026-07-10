use leptos::prelude::*;

use crate::features::chat::components::ChatView;
use crate::state::title;

#[component]
pub fn ChatPage() -> impl IntoView {
    title::set_page_title("Chat");
    view! { <ChatView /> }
}
