use leptos::prelude::*;

use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::cluster::Cluster;
use crate::primitives::stack::{Gap, Stack};
use crate::theme::{class, color, radius, space, typography};

pub struct NavItem {
    pub label: &'static str,
    pub href: &'static str,
    pub count: Option<u32>,
    pub active: bool,
}

pub struct NavSection {
    pub label: &'static str,
    pub items: Vec<NavItem>,
}

#[must_use]
pub fn sidebar_sections() -> Vec<NavSection> {
    vec![
        NavSection {
            label: "Workspace",
            items: vec![
                NavItem { label: "Home", href: "/dashboard", count: None, active: true },
                NavItem { label: "Inbox", href: "#", count: Some(3), active: false },
                NavItem { label: "Announcements", href: "#", count: None, active: false },
            ],
        },
        NavSection {
            label: "Work",
            items: vec![
                NavItem { label: "Groups", href: "#", count: None, active: false },
                NavItem { label: "Projects", href: "#", count: Some(6), active: false },
                NavItem { label: "Requests", href: "#", count: Some(7), active: false },
                NavItem { label: "IT tickets", href: "#", count: Some(4), active: false },
                NavItem { label: "Chat", href: "#", count: None, active: false },
                NavItem { label: "Files", href: "#", count: None, active: false },
            ],
        },
        NavSection {
            label: "Admin",
            items: vec![
                NavItem { label: "HR lifecycle", href: "#", count: None, active: false },
                NavItem { label: "Permissions", href: "#", count: None, active: false },
                NavItem { label: "Settings", href: "#", count: None, active: false },
            ],
        },
    ]
}

#[component]
pub fn Wordmark() -> impl IntoView {
    let cls = class(format!(
        "font-family: {ff}; font-size: 16px; font-weight: {fw}; \
         color: {c}; letter-spacing: -0.02em;",
        ff = typography::FONT_SANS,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    view! { <span class=cls>"Portal"</span> }
}

#[component]
pub fn SidebarNav() -> impl IntoView {
    let nav_cls = class(format!(
        "display: flex; flex-direction: column; gap: {g}; padding: {p};",
        g = space::D5,
        p = space::D3,
    ));
    let eyebrow_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         text-transform: uppercase; letter-spacing: 0.08em; color: {c}; \
         padding: 0 {p}; margin-bottom: {mb};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_EYEBROW,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_FAINT,
        p = space::D3,
        mb = space::D1,
    ));
    let item_cls = class(format!(
        "display: flex; align-items: center; gap: {g}; \
         padding: 6px {p}; height: 30px; border-radius: {r}; \
         color: {c}; font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         text-decoration: none; transition: background 120ms ease, color 120ms ease; \
         &:hover {{ background: {bh}; color: {ct}; }}",
        g = space::D2,
        p = space::D3,
        r = radius::SM,
        c = color::TEXT_MUTED,
        ct = color::TEXT,
        bh = color::BG_HOVER,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_MEDIUM,
    ));
    let active_cls = class(format!(
        "background: {bg} !important; color: {c} !important;",
        bg = color::ACCENT_BG,
        c = color::ACCENT,
    ));
    let count_cls = class(format!(
        "margin-left: auto; font-size: 11px; padding: 1px 6px; \
         background: {bg}; color: {c}; border-radius: {r}; \
         font-weight: {fw}; min-width: 18px; text-align: center;",
        bg = color::BG_SUNKEN,
        c = color::TEXT_MUTED,
        r = radius::PILL,
        fw = typography::WEIGHT_MEDIUM,
    ));
    let header_cls = class(format!(
        "display: flex; align-items: center; justify-content: space-between; \
         padding: {p2} {p3} {p4};",
        p2 = space::D2,
        p3 = space::D3,
        p4 = space::D4,
    ));
    let row_cls = class("display: flex; flex-direction: column; gap: 1px;");

    let sections = sidebar_sections();

    view! {
        <div>
            <div class=header_cls>
                <Wordmark />
            </div>
            <nav class=nav_cls>
                {sections.into_iter().map(|sec| {
                    let eyebrow_cls = eyebrow_cls.clone();
                    let item_cls = item_cls.clone();
                    let active_cls = active_cls.clone();
                    let count_cls = count_cls.clone();
                    let row_cls = row_cls.clone();
                    view! {
                        <div>
                            <div class=eyebrow_cls>{sec.label}</div>
                            <div class=row_cls>
                                {sec.items.into_iter().map(|item| {
                                    let cls = if item.active {
                                        format!("{item_cls} {active_cls}")
                                    } else {
                                        item_cls.clone()
                                    };
                                    let count_cls = count_cls.clone();
                                    view! {
                                        <a class=cls href=item.href>
                                            <span style="flex:1;">{item.label}</span>
                                            {item.count.map(|n| view! {
                                                <span class=count_cls.clone()>{n}</span>
                                            })}
                                        </a>
                                    }
                                }).collect_view()}
                            </div>
                        </div>
                    }
                }).collect_view()}
            </nav>
        </div>
    }
}

#[component]
pub fn Topbar() -> impl IntoView {
    let bar = class(format!(
        "height: {h}; flex-shrink: 0; \
         display: flex; align-items: center; justify-content: space-between; \
         padding: 0 {p}; border-bottom: 1px solid {b}; background: {bg}; \
         position: sticky; top: 0; z-index: 10;",
        h = space::TOPBAR_H,
        p = space::D6,
        b = color::BORDER,
        bg = color::BG,
    ));
    let crumb = class(format!(
        "display: flex; align-items: center; gap: {g}; \
         font-family: {ff}; font-size: {fs}; color: {c};",
        g = space::D2,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));
    let strong = class(format!(
        "color: {c}; font-weight: {fw};",
        c = color::TEXT_STRONG,
        fw = typography::WEIGHT_SEMIBOLD,
    ));

    view! {
        <div class=bar>
            <div class=crumb>
                <span>"Workspace"</span>
                <span>"›"</span>
                <span class=strong>"Home"</span>
            </div>
            <Cluster gap=Gap::Sm>
                <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm>"Help"</Button>
                <Button variant=ButtonVariant::Primary size=ButtonSize::Sm>"+ New request"</Button>
            </Cluster>
        </div>
    }
}

#[component]
pub fn Hero(#[prop(into)] greeting: String, #[prop(into)] subtitle: String) -> impl IntoView {
    let wrap = class(format!(
        "display: grid; grid-template-columns: 1fr auto; gap: {g}; \
         align-items: end; margin-bottom: {mb};",
        g = space::D6,
        mb = space::D6,
    ));
    let eyebrow = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         text-transform: uppercase; letter-spacing: 0.08em; \
         color: {c}; margin-bottom: {mb};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_EYEBROW,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_FAINT,
        mb = space::D2,
    ));
    let h1 = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         color: {c}; margin: 0; letter-spacing: -0.02em;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H1,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let p = class(format!(
        "font-family: {ff}; font-size: 14.5px; color: {c}; \
         margin: 6px 0 0; max-width: 560px;",
        ff = typography::FONT_SANS,
        c = color::TEXT_MUTED,
    ));

    view! {
        <div class=wrap>
            <div>
                <div class=eyebrow>"Today"</div>
                <h1 class=h1>{greeting}</h1>
                <p class=p>{subtitle}</p>
            </div>
            <Cluster gap=Gap::Sm>
                <Button variant=ButtonVariant::Secondary>"Filters"</Button>
                <Button variant=ButtonVariant::Primary>"+ New request"</Button>
            </Cluster>
        </div>
    }
}

pub struct StatTile {
    pub label: &'static str,
    pub value: u32,
    pub delta: &'static str,
}

#[component]
pub fn StatTiles() -> impl IntoView {
    let grid = class(format!(
        "display: grid; grid-template-columns: repeat(4, 1fr); gap: {g}; margin-bottom: {mb};",
        g = space::D4,
        mb = space::D5,
    ));
    let label_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         text-transform: uppercase; letter-spacing: 0.08em; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_EYEBROW,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_FAINT,
    ));
    let value_cls = class(format!(
        "font-family: {ff}; font-size: 26px; font-weight: {fw}; \
         letter-spacing: -0.02em; color: {c};",
        ff = typography::FONT_SANS,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let delta_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    let row_cls = class("display: flex; align-items: baseline; gap: 8px;");

    let tiles = vec![
        StatTile { label: "Pending review", value: 3, delta: "+1 today" },
        StatTile { label: "Open requests", value: 7, delta: "2 high priority" },
        StatTile { label: "Open IT tickets", value: 4, delta: "1 in triage" },
        StatTile { label: "Group members", value: 11, delta: "1 starting Mon" },
    ];

    view! {
        <div class=grid>
            {tiles.into_iter().map(|t| {
                let label_cls = label_cls.clone();
                let value_cls = value_cls.clone();
                let delta_cls = delta_cls.clone();
                let row_cls = row_cls.clone();
                view! {
                    <Card>
                        <Stack gap=Gap::Md>
                            <div class=label_cls>{t.label}</div>
                            <div class=row_cls>
                                <span class=value_cls>{t.value}</span>
                                <span class=delta_cls>{t.delta}</span>
                            </div>
                        </Stack>
                    </Card>
                }
            }).collect_view()}
        </div>
    }
}

#[component]
pub fn UserCard(#[prop(into)] name: String, #[prop(into)] role: String) -> impl IntoView {
    let name_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let role_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));

    view! {
        <Cluster gap=Gap::Sm>
            <Avatar name=name.clone() size=AvatarSize::Md tone=0 />
            <div>
                <div class=name_cls>{name}</div>
                <div class=role_cls>{role}</div>
            </div>
        </Cluster>
    }
}
