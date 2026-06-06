use leptos::prelude::*;

use crate::features::audit::components::AuditLogIndex;
use crate::features::home::shell::AuthedPage;

#[component]
pub fn AuditPage() -> impl IntoView {
    view! {
        <AuthedPage title="Audit log">
            <AuditLogIndex />
        </AuthedPage>
    }
}
