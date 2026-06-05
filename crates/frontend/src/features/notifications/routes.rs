use leptos::prelude::*;

use crate::features::home::shell::AuthedPage;
use crate::features::notifications::components::InboxIndex;

#[component]
pub fn InboxPage() -> impl IntoView {
    view! {
        <AuthedPage title="Inbox">
            <InboxIndex />
        </AuthedPage>
    }
}
