use leptos::prelude::*;

use crate::features::announcements::components::AnnouncementsIndex;
use crate::features::home::shell::AuthedPage;

#[component]
pub fn AnnouncementsPage() -> impl IntoView {
    view! {
        <AuthedPage title="Announcements">
            <AnnouncementsIndex />
        </AuthedPage>
    }
}
