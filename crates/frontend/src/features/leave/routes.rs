use leptos::prelude::*;

use crate::features::home::shell::AuthedPage;
use crate::features::leave::components::MyLeave;

#[component]
pub fn LeavePage() -> impl IntoView {
    view! {
        <AuthedPage title="My leave">
            <MyLeave />
        </AuthedPage>
    }
}
