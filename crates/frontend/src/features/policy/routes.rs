use leptos::prelude::*;

use crate::features::policy::components::PolicyForm;
use crate::state::title;

#[component]
pub fn PolicyPage() -> impl IntoView {
    title::set_page_title("Attendance policy");
    view! { <PolicyForm /> }
}
