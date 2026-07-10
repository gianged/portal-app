use leptos::prelude::*;

use crate::features::audit::components::AuditLogIndex;
use crate::state::title;

#[component]
pub fn AuditPage() -> impl IntoView {
    title::set_page_title("Audit log");
    view! { <AuditLogIndex /> }
}
