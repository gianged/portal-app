use leptos::{prelude::*, task};
use leptos_router::components::A;
use shared::dto::{
    chat::ChannelSummaryDto, group::GroupDto, request::RequestDto, ticket::TicketDto,
};

use crate::features::auth::components::RequireAuth;
use crate::features::home::api as home_api;
use crate::features::home::components::{Hero, Wordmark};
use crate::features::home::panels::{
    ChannelsPanel, Loadable, RequestsPanel, StatTiles, TicketsPanel,
};
use crate::features::home::shell::{AppShell, AuthedPage};
use crate::primitives::card::Card;
use crate::primitives::center::Center;
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::IconName;
use crate::primitives::stack::{Gap, Stack};
use crate::state::auth::AuthState;
use crate::theme::{self, color, space, typography};

#[component]
pub fn LandingPage() -> impl IntoView {
    let wrap = theme::class("width: 100%; max-width: 420px;");
    let title = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         color: {c}; margin: 0; letter-spacing: -0.02em;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H1,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let subtitle = theme::class(format!(
        "font-family: {ff}; font-size: 14.5px; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        c = color::TEXT_MUTED,
    ));
    let cta = theme::class(format!(
        "display: inline-flex; align-items: center; justify-content: center; \
         height: {h}; padding: 0 {p}; background: {bg}; color: {fg}; \
         border-radius: 8px; text-decoration: none; \
         font-family: {ff}; font-size: 14.5px; font-weight: {fw};",
        h = space::BTN_H_LG,
        p = space::D5,
        bg = color::ACCENT,
        fg = color::TEXT_ON_ACCENT,
        ff = typography::FONT_SANS,
        fw = typography::WEIGHT_MEDIUM,
    ));

    view! {
        <Center>
            <div class=wrap>
                <Card padding=format!("{} {}", space::D8, space::D7)>
                    <Stack gap=Gap::Xl>
                        <Wordmark />
                        <Stack gap=Gap::Sm>
                            <h1 class=title>"Welcome to Portal."</h1>
                            <p class=subtitle>
                                "Internal portal for projects, requests, tickets, and chat."
                            </p>
                        </Stack>
                        <A href="/login" attr:class=cta>"Sign in"</A>
                    </Stack>
                </Card>
            </div>
        </Center>
    }
}

#[component]
pub fn DashboardPage() -> impl IntoView {
    view! {
        <RequireAuth>
            <AppShell title="Home">
                <DashboardContent />
            </AppShell>
        </RequireAuth>
    }
}

#[component]
fn DashboardContent() -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");

    // Each loadable starts None and is filled by a one-shot fetch; RequireAuth guarantees auth.user is present.
    let requests: Loadable<Vec<RequestDto>> = RwSignal::new(None);
    let tickets: Loadable<Vec<TicketDto>> = RwSignal::new(None);
    let channels: Loadable<Vec<ChannelSummaryDto>> = RwSignal::new(None);
    let groups: Loadable<Vec<GroupDto>> = RwSignal::new(None);

    task::spawn_local(async move { requests.set(Some(home_api::my_requests().await)) });
    task::spawn_local(async move { tickets.set(Some(home_api::my_tickets().await)) });
    task::spawn_local(async move { channels.set(Some(home_api::channels().await)) });
    task::spawn_local(async move { groups.set(Some(home_api::groups().await)) });

    let greeting = auth.user.with(|u| {
        u.as_ref().map_or_else(
            || "Welcome back.".to_owned(),
            |user| format!("Welcome back, {}.", first_name(&user.name)),
        )
    });

    let grid = theme::class(format!(
        "display: grid; grid-template-columns: minmax(0, 1fr) 360px; \
         gap: {g}; align-items: start;",
        g = space::D5,
    ));

    view! {
        <Hero
            greeting=greeting
            subtitle="Here's what's happening across your workspace today."
        />
        <StatTiles requests=requests tickets=tickets channels=channels groups=groups />
        <div class=grid>
            <RequestsPanel requests=requests />
            <Stack gap=Gap::Md>
                <TicketsPanel tickets=tickets />
                <ChannelsPanel channels=channels />
            </Stack>
        </div>
    }
}

fn first_name(full: &str) -> String {
    full.split_whitespace().next().unwrap_or(full).to_owned()
}

/// File access is via request attachments; there is no standalone file browser (backend is download-by-key only).
#[component]
pub fn FilesPage() -> impl IntoView {
    view! {
        <AuthedPage title="Files">
            <EmptyState
                icon=IconName::Folder
                title="Files live with their work item"
                description="Attachments are uploaded and downloaded from the request they belong to. \
                             There's no separate file browser."
            />
        </AuthedPage>
    }
}

/// Access control is resolved server-side via OpenFGA; there is no permission-editing surface in the UI.
#[component]
pub fn PermissionsPage() -> impl IntoView {
    view! {
        <AuthedPage title="Permissions">
            <EmptyState
                icon=IconName::Lock
                title="Permissions follow the org graph"
                description="Who can see and do what is derived from group roles and project \
                             membership via OpenFGA, not edited directly here."
            />
        </AuthedPage>
    }
}
