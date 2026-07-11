use leptos::ev::MouseEvent;
use leptos::prelude::*;

use shared::dto::user::UserRole;

use crate::features::reports::archive::ReportArchive;
use crate::features::reports::monthly::MonthlyTab;
use crate::features::reports::staff::StaffMonthlyTab;
use crate::features::reports::yearly::YearlyTab;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::tabs::{Tab, Tabs};
use crate::state::auth::AuthState;
use crate::state::title;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReportTab {
    Monthly,
    Yearly,
    Staff,
}

#[component]
pub fn ReportsPage() -> impl IntoView {
    title::set_page_title("Reports");
    view! { <ReportsIndex /> }
}

#[component]
fn ReportsIndex() -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let is_admin = Signal::derive(move || {
        auth.user.with(|u| {
            u.as_ref()
                .is_some_and(|u| matches!(u.role, UserRole::Director | UserRole::Hr))
        })
    });
    let tab = RwSignal::new(ReportTab::Monthly);
    // Bumped by the generate handlers so the archive below re-fetches.
    let archive_refresh = RwSignal::new(0u32);

    view! {
        {move || {
            if !is_admin.get() {
                // Non-admins see only their own per-staff monthly report.
                return view! {
                    <Stack gap=Gap::Lg>
                        <StaffMonthlyTab />
                    </Stack>
                }
                .into_any();
            }
            let monthly_active = Signal::derive(move || tab.get() == ReportTab::Monthly);
            let yearly_active = Signal::derive(move || tab.get() == ReportTab::Yearly);
            let staff_active = Signal::derive(move || tab.get() == ReportTab::Staff);
            view! {
                <Stack gap=Gap::Lg>
                    <Tabs>
                        <Tab
                            active=monthly_active
                            on_click=Callback::new(move |_: MouseEvent| tab.set(ReportTab::Monthly))
                        >
                            "Monthly"
                        </Tab>
                        <Tab
                            active=yearly_active
                            on_click=Callback::new(move |_: MouseEvent| tab.set(ReportTab::Yearly))
                        >
                            "Yearly"
                        </Tab>
                        <Tab
                            active=staff_active
                            on_click=Callback::new(move |_: MouseEvent| tab.set(ReportTab::Staff))
                        >
                            "Per staff"
                        </Tab>
                    </Tabs>
                    {move || match tab.get() {
                        ReportTab::Monthly => view! { <MonthlyTab refresh=archive_refresh /> }.into_any(),
                        ReportTab::Yearly => view! { <YearlyTab refresh=archive_refresh /> }.into_any(),
                        ReportTab::Staff => view! { <StaffMonthlyTab /> }.into_any(),
                    }}
                    <ReportArchive refresh=archive_refresh />
                </Stack>
            }
            .into_any()
        }}
    }
}
