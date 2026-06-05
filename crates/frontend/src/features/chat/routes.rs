use leptos::prelude::*;

use crate::features::chat::components::ChatView;
use crate::features::home::shell::AuthedPage;

#[component]
pub fn ChatPage() -> impl IntoView {
    view! {
        <AuthedPage title="Chat">
            <ChatView />
        </AuthedPage>
    }
}
