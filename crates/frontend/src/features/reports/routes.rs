use leptos::ev::MouseEvent;
use leptos::prelude::*;

use shared::dto::user::UserRole;

use crate::features::home::shell::AuthedPage;
use crate::features::reports::archive::ReportArchive;
use crate::features::reports::monthly::MonthlyTab;
use crate::features::reports::yearly::YearlyTab;
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::IconName;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::tabs::{Tab, Tabs};
use crate::state::auth::AuthState;
use crate::theme::{class, color, typography};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReportTab {
    Monthly,
    Yearly,
}

#[component]
pub fn ReportsPage() -> impl IntoView {
    view! {
        <AuthedPage title="Reports">
            <ReportsIndex />
        </AuthedPage>
    }
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

    view! {
        {move || {
            if !is_admin.get() {
                return view! {
                    <EmptyState
                        icon=IconName::Lock
                        title="Restricted"
                        description="Reports are available to Directors and HR only."
                    />
                }
                .into_any();
            }
            let monthly_active = Signal::derive(move || tab.get() == ReportTab::Monthly);
            let yearly_active = Signal::derive(move || tab.get() == ReportTab::Yearly);
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
                    </Tabs>
                    {move || {
                        if tab.get() == ReportTab::Monthly {
                            view! { <MonthlyTab /> }.into_any()
                        } else {
                            view! { <YearlyTab /> }.into_any()
                        }
                    }}
                    <ReportArchive />
                </Stack>
            }
            .into_any()
        }}
    }
}

/// A small label + figure tile used across the report tabs.
#[must_use]
pub(crate) fn metric(label: &str, value: String) -> AnyView {
    let wrap = class("display: flex; flex-direction: column; gap: 2px; min-width: 90px;");
    let value_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H3,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let label_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    view! {
        <div class=wrap>
            <div class=value_cls>{value}</div>
            <div class=label_cls>{label.to_owned()}</div>
        </div>
    }
    .into_any()
}

/// A section heading used inside report cards.
#[must_use]
pub(crate) fn section_title(text: &str) -> AnyView {
    let cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    view! { <div class=cls>{text.to_owned()}</div> }.into_any()
}

#[must_use]
pub(crate) fn month_name(m: u8) -> &'static str {
    match m {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        _ => "December",
    }
}

#[must_use]
pub(crate) fn month_abbr(m: u8) -> &'static str {
    match m {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        _ => "Dec",
    }
}
