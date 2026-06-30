use leptos::prelude::*;

use crate::features::home::shell::AuthedPage;
use crate::features::overtime::components::{Approvals, Overtime};

#[component]
pub fn OvertimePage() -> impl IntoView {
    view! {
        <AuthedPage title="Overtime">
            <Overtime />
        </AuthedPage>
    }
}

#[component]
pub fn OvertimeApprovalsPage() -> impl IntoView {
    view! {
        <AuthedPage title="Overtime approvals">
            <Approvals />
        </AuthedPage>
    }
}
