use leptos::prelude::*;

use crate::features::announcements::components::AnnouncementsIndex;
use crate::state::title;

#[component]
pub fn AnnouncementsPage() -> impl IntoView {
    title::set_page_title("Announcements");
    view! { <AnnouncementsIndex /> }
}
