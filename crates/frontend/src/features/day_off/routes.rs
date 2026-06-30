use leptos::prelude::*;

use crate::features::day_off::components::{Approvals, TimeOff};
use crate::features::home::shell::AuthedPage;

#[component]
pub fn TimeOffPage() -> impl IntoView {
    view! {
        <AuthedPage title="Time off">
            <TimeOff />
        </AuthedPage>
    }
}

#[component]
pub fn LeaveApprovalsPage() -> impl IntoView {
    view! {
        <AuthedPage title="Leave approvals">
            <Approvals />
        </AuthedPage>
    }
}
