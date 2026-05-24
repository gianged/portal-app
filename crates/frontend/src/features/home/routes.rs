use leptos::prelude::*;
use leptos_router::components::A;

use crate::features::home::components::{Hero, SidebarNav, StatTiles, Topbar, Wordmark};
use crate::primitives::card::Card;
use crate::primitives::center::Center;
use crate::primitives::sidebar::SidebarLayout;
use crate::primitives::stack::{Gap, Stack};
use crate::theme::{class, color, space, typography};

#[component]
pub fn LandingPage() -> impl IntoView {
    let wrap = class("width: 100%; max-width: 420px;");
    let title = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         color: {c}; margin: 0; letter-spacing: -0.02em;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H1,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let subtitle = class(format!(
        "font-family: {ff}; font-size: 14.5px; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        c = color::TEXT_MUTED,
    ));
    let cta = class(format!(
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
    let main_cls = class(format!(
        "padding: {p1} {p2} {p3}; max-width: {mw}; width: 100%; margin: 0 auto;",
        p1 = space::D6,
        p2 = space::D7,
        p3 = space::D8,
        mw = space::CONTENT_MAX_W,
    ));

    let side = view! { <SidebarNav /> }.into_any();
    let main = view! {
        <Topbar />
        <main class=main_cls>
            <Hero
                greeting="Good morning."
                subtitle="You have 3 requests waiting on your review and 1 ticket in triage for your group."
            />
            <StatTiles />
        </main>
    }
    .into_any();

    view! { <SidebarLayout side=side main=main /> }
}
