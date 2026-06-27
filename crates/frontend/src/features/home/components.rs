#![allow(dead_code)] // TODO: UserCard unused, I will see it

use leptos::{prelude::*, task::spawn_local};
use leptos_router::{NavigateOptions, components::A, hooks::{use_location, use_navigate}};

use crate::features::auth::api as auth_api;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::button::{Button, ButtonVariant};
use crate::primitives::cluster::Cluster;
use crate::primitives::dropdown::Dropdown;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::stack::Gap;
use crate::state::auth::AuthState;
use crate::state::notifications::NotificationsState;
use crate::state::theme::ThemeState;
use crate::theme::{self, color, radius, space, typography};
use crate::util::format;

pub struct NavItem {
    pub label: &'static str,
    pub href: &'static str,
    pub icon: IconName,
    pub count: Option<u32>,
    /// `false` for sections without a page yet (rendered muted, non-navigating).
    pub enabled: bool,
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
                NavItem {
                    label: "Home",
                    href: "/dashboard",
                    icon: IconName::Spark,
                    count: None,
                    enabled: true,
                },
                NavItem {
                    label: "Inbox",
                    href: "/inbox",
                    icon: IconName::Inbox,
                    count: None,
                    enabled: true,
                },
                NavItem {
                    label: "Announcements",
                    href: "/announcements",
                    icon: IconName::Megaphone,
                    count: None,
                    enabled: true,
                },
            ],
        },
        NavSection {
            label: "Work",
            items: vec![
                NavItem {
                    label: "Groups",
                    href: "/groups",
                    icon: IconName::Users,
                    count: None,
                    enabled: true,
                },
                NavItem {
                    label: "Projects",
                    href: "/projects",
                    icon: IconName::Folder,
                    count: None,
                    enabled: true,
                },
                NavItem {
                    label: "Requests",
                    href: "/requests",
                    icon: IconName::Doc,
                    count: None,
                    enabled: true,
                },
                NavItem {
                    label: "IT tickets",
                    href: "/tickets",
                    icon: IconName::Ticket,
                    count: None,
                    enabled: true,
                },
                NavItem {
                    label: "Chat",
                    href: "/chat",
                    icon: IconName::Chat,
                    count: None,
                    enabled: true,
                },
                NavItem {
                    label: "Files",
                    href: "/files",
                    icon: IconName::Folder,
                    count: None,
                    enabled: true,
                },
            ],
        },
        NavSection {
            label: "Admin",
            items: vec![
                NavItem {
                    label: "People",
                    href: "/users",
                    icon: IconName::Building,
                    count: None,
                    enabled: true,
                },
                NavItem {
                    label: "Permissions",
                    href: "/permissions",
                    icon: IconName::Lock,
                    count: None,
                    enabled: true,
                },
                NavItem {
                    label: "Audit log",
                    href: "/audit",
                    icon: IconName::Clock,
                    count: None,
                    enabled: true,
                },
                NavItem {
                    label: "Reports",
                    href: "/reports",
                    icon: IconName::Doc,
                    count: None,
                    enabled: true,
                },
            ],
        },
    ]
}

#[component]
pub fn Wordmark() -> impl IntoView {
    let cls = theme::class(format!(
        "display: inline-flex; align-items: center; gap: {g}; font-family: {ff}; \
         font-size: 16px; font-weight: {fw}; color: {c}; letter-spacing: -0.02em;",
        g = space::D2,
        ff = typography::FONT_SANS,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let mark = theme::class(format!(
        "display: inline-flex; align-items: center; justify-content: center; \
         width: 22px; height: 22px; border-radius: {r}; background: {bg}; color: {fg};",
        r = radius::SM,
        bg = color::ACCENT,
        fg = color::TEXT_ON_ACCENT,
    ));
    view! {
        <span class=cls>
            <span class=mark><Icon name=IconName::Spark size=14 /></span>
            "Portal"
        </span>
    }
}

#[component]
pub fn SidebarNav() -> impl IntoView {
    let pathname = use_location().pathname;
    let notifications = use_context::<NotificationsState>().expect("NotificationsState context");

    let header_cls = theme::class(format!(
        "display: flex; align-items: center; justify-content: space-between; padding: {p2} {p3} {p4};",
        p2 = space::D2,
        p3 = space::D3,
        p4 = space::D4,
    ));
    let nav_cls = theme::class(format!(
        "display: flex; flex-direction: column; gap: {g}; padding: {p};",
        g = space::D5,
        p = space::D3,
    ));
    let eyebrow_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; text-transform: uppercase; \
         letter-spacing: 0.08em; color: {c}; padding: 0 {p}; margin-bottom: {mb};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_EYEBROW,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_FAINT,
        p = space::D3,
        mb = space::D1,
    ));
    let item_cls = theme::class(format!(
        "display: flex; align-items: center; gap: {g}; padding: 6px {p}; height: 32px; \
         border-radius: {r}; color: {c}; font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         text-decoration: none; transition: background 120ms ease, color 120ms ease; \
         &:hover {{ background: {bh}; color: {ct}; }}",
        g = space::D2,
        p = space::D3,
        r = radius::SM,
        c = color::TEXT_MUTED,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_MEDIUM,
        bh = color::BG_HOVER,
        ct = color::TEXT,
    ));
    let active_cls = theme::class(format!(
        "background: {bg} !important; color: {c} !important;",
        bg = color::ACCENT_BG,
        c = color::ACCENT,
    ));
    let disabled_cls = theme::class(format!(
        "display: flex; align-items: center; gap: {g}; padding: 6px {p}; height: 32px; \
         border-radius: {r}; color: {c}; font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         cursor: default;",
        g = space::D2,
        p = space::D3,
        r = radius::SM,
        c = color::TEXT_FAINT,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_MEDIUM,
    ));
    let count_cls = theme::class(format!(
        "font-size: 11px; padding: 1px 6px; background: {bg}; color: {c}; border-radius: {r}; \
         font-weight: {fw}; min-width: 18px; text-align: center;",
        bg = color::BG_SUNKEN,
        c = color::TEXT_MUTED,
        r = radius::PILL,
        fw = typography::WEIGHT_MEDIUM,
    ));
    let soon_cls = theme::class(format!(
        "font-size: 10px; letter-spacing: 0.04em; text-transform: uppercase; color: {c};",
        c = color::TEXT_FAINT,
    ));
    let grow_cls = theme::class("flex: 1; min-width: 0;");
    let row_cls = theme::class("display: flex; flex-direction: column; gap: 1px;");

    let sections = sidebar_sections();

    view! {
        <div>
            <div class=header_cls><Wordmark /></div>
            <nav class=nav_cls>
                {sections.into_iter().map(|sec| {
                    let eyebrow_cls = eyebrow_cls.clone();
                    let row_cls = row_cls.clone();
                    let item_cls = item_cls.clone();
                    let active_cls = active_cls.clone();
                    let disabled_cls = disabled_cls.clone();
                    let count_cls = count_cls.clone();
                    let soon_cls = soon_cls.clone();
                    let grow_cls = grow_cls.clone();
                    view! {
                        <div>
                            <div class=eyebrow_cls>{sec.label}</div>
                            <div class=row_cls>
                                {sec.items.into_iter().map(|item| {
                                    let count_cls = count_cls.clone();
                                    let grow_cls = grow_cls.clone();
                                    if item.enabled {
                                        let base = item_cls.clone();
                                        let act = active_cls.clone();
                                        let href = item.href;
                                        let cls = move || if pathname.get() == href {
                                            format!("{base} {act}")
                                        } else {
                                            base.clone()
                                        };
                                        // Inbox count tracks the live unread badge; others use the static seed.
                                        let count_view = if item.href == "/inbox" {
                                            let cc = count_cls.clone();
                                            view! {
                                                <Show when=move || notifications.unread.get() != 0 fallback=|| ()>
                                                    <span class=cc.clone()>{move || notifications.unread.get()}</span>
                                                </Show>
                                            }.into_any()
                                        } else {
                                            match item.count {
                                                Some(n) => view! { <span class=count_cls.clone()>{n}</span> }.into_any(),
                                                None => ().into_any(),
                                            }
                                        };
                                        view! {
                                            <A href=item.href attr:class=cls>
                                                <Icon name=item.icon size=16 />
                                                <span class=grow_cls>{item.label}</span>
                                                {count_view}
                                            </A>
                                        }.into_any()
                                    } else {
                                        let soon_cls = soon_cls.clone();
                                        view! {
                                            <div class=disabled_cls.clone()>
                                                <Icon name=item.icon size=16 />
                                                <span class=grow_cls>{item.label}</span>
                                                <span class=soon_cls>"soon"</span>
                                            </div>
                                        }.into_any()
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
pub fn Topbar(#[prop(into)] title: String) -> impl IntoView {
    let bar = theme::class(format!(
        "height: {h}; flex-shrink: 0; display: flex; align-items: center; \
         justify-content: space-between; padding: 0 {p}; border-bottom: 1px solid {b}; \
         background: {bg}; position: sticky; top: 0; z-index: 10;",
        h = space::TOPBAR_H,
        p = space::D6,
        b = color::BORDER,
        bg = color::BG,
    ));
    let crumb = theme::class(format!(
        "display: flex; align-items: center; gap: {g}; font-family: {ff}; \
         font-size: {fs}; color: {c};",
        g = space::D2,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));
    let strong = theme::class(format!(
        "color: {c}; font-weight: {fw};",
        c = color::TEXT_STRONG,
        fw = typography::WEIGHT_SEMIBOLD,
    ));
    let divider = theme::class(format!(
        "width: 1px; height: 20px; background: {b};",
        b = color::BORDER,
    ));

    view! {
        <div class=bar>
            <div class=crumb>
                <span>"Workspace"</span>
                <Icon name=IconName::ChevronRight size=12 />
                <span class=strong>{title}</span>
            </div>
            <Cluster gap=Gap::Sm>
                <ThemeToggle />
                <NotificationsBell />
                <span class=divider></span>
                <UserMenu />
            </Cluster>
        </div>
    }
}

#[component]
fn ThemeToggle() -> impl IntoView {
    let theme = use_context::<ThemeState>().expect("ThemeState context");
    let on_click = Callback::new(move |_| theme.toggle());
    view! {
        <Button variant=ButtonVariant::Icon on_click=on_click>
            {move || if theme.is_dark() {
                view! { <Icon name=IconName::Sun /> }.into_any()
            } else {
                view! { <Icon name=IconName::Moon /> }.into_any()
            }}
        </Button>
    }
}

#[component]
fn NotificationsBell() -> impl IntoView {
    let notifications = use_context::<NotificationsState>().expect("NotificationsState context");
    let link = theme::class(format!(
        "position: relative; display: inline-flex; align-items: center; justify-content: center; \
         width: {h}; height: {h}; border-radius: {r}; color: {c}; text-decoration: none; \
         transition: background 120ms ease, color 120ms ease; \
         &:hover {{ background: {bh}; color: {ct}; }}",
        h = space::BTN_H,
        r = radius::MD,
        c = color::TEXT_MUTED,
        bh = color::BG_HOVER,
        ct = color::TEXT,
    ));
    let badge = theme::class(format!(
        "position: absolute; top: 2px; right: 2px; min-width: 16px; height: 16px; \
         padding: 0 4px; display: inline-flex; align-items: center; justify-content: center; \
         background: {bg}; color: #fff; border-radius: {r}; font-size: 10px; font-weight: {fw}; \
         border: 2px solid {bd};",
        bg = color::DANGER,
        r = radius::PILL,
        fw = typography::WEIGHT_SEMIBOLD,
        bd = color::BG,
    ));
    view! {
        <A href="/inbox" attr:class=link>
            <Icon name=IconName::Bell />
            <Show when=move || notifications.unread.get() != 0 fallback=|| ()>
                <span class=badge.clone()>{move || notifications.unread.get()}</span>
            </Show>
        </A>
    }
}

#[component]
fn UserMenu() -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let navigate = use_navigate();

    let trigger_cls = theme::class(format!(
        "display: inline-flex; align-items: center; gap: {g}; padding: 4px 6px; \
         border-radius: {r}; cursor: pointer; &:hover {{ background: {bh}; }}",
        g = space::D2,
        r = radius::SM,
        bh = color::BG_HOVER,
    ));
    let name_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let role_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    let chevron_cls = theme::class(format!(
        "color: {c}; display: inline-flex;",
        c = color::TEXT_FAINT
    ));
    let meta_cls = theme::class(format!(
        "padding: {p}; border-bottom: 1px solid {b}; margin-bottom: {mb};",
        p = space::D2,
        b = color::BORDER,
        mb = space::D1,
    ));
    let email_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; margin-top: 2px;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_FAINT,
    ));
    let profile_link_cls = theme::class(format!(
        "display: flex; align-items: center; gap: {g}; padding: 6px {p}; border-radius: {r}; \
         font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; text-decoration: none; \
         &:hover {{ background: {bh}; }}",
        g = space::D2,
        p = space::D2,
        r = radius::SM,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_MEDIUM,
        c = color::TEXT,
        bh = color::BG_HOVER,
    ));

    let on_logout = Callback::new(move |_| {
        let navigate = navigate.clone();
        spawn_local(async move {
            let _ = auth_api::logout().await;
            auth.clear();
            navigate("/login", NavigateOptions::default());
        });
    });

    // The user is always present inside the authed shell, so resolve the profile href once (untracked).
    let profile_href = auth.user.with_untracked(|u| {
        u.as_ref()
            .map_or_else(|| "/users".to_owned(), |x| format!("/users/{}", x.id.0))
    });

    let menu_name_cls = name_cls.clone();
    let trigger = view! {
        <div class=trigger_cls>
            {move || {
                let (name, role) = auth.user.with(|u| match u {
                    Some(user) => (user.name.clone(), user.role.label().to_owned()),
                    None => ("Account".to_owned(), String::new()),
                });
                let tone = format::tone_for(&name);
                let name_cls = name_cls.clone();
                let role_cls = role_cls.clone();
                view! {
                    <Avatar name=name.clone() size=AvatarSize::Sm tone=tone />
                    <div>
                        <div class=name_cls>{name}</div>
                        <div class=role_cls>{role}</div>
                    </div>
                }
            }}
            <span class=chevron_cls><Icon name=IconName::ChevronDown size=14 /></span>
        </div>
    }
    .into_any();

    view! {
        <Dropdown trigger=trigger>
            <div class=meta_cls.clone()>
                <div class=menu_name_cls.clone()>
                    {move || auth.user.with(|u| u.as_ref().map_or_else(String::new, |x| x.name.clone()))}
                </div>
                <div class=email_cls.clone()>
                    {move || auth.user.with(|u| u.as_ref().map_or_else(String::new, |x| x.email.clone()))}
                </div>
            </div>
            <A href=profile_href.clone() attr:class=profile_link_cls.clone()>
                <Icon name=IconName::Users size=14 />
                "Profile"
            </A>
            <Button variant=ButtonVariant::Ghost full_width=true on_click=on_logout>
                "Sign out"
            </Button>
        </Dropdown>
    }
}

#[component]
pub fn Hero(#[prop(into)] greeting: String, #[prop(into)] subtitle: String) -> impl IntoView {
    let wrap = theme::class(format!(
        "display: grid; grid-template-columns: 1fr auto; gap: {g}; \
         align-items: end; margin-bottom: {mb};",
        g = space::D6,
        mb = space::D6,
    ));
    let eyebrow = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         text-transform: uppercase; letter-spacing: 0.08em; \
         color: {c}; margin-bottom: {mb};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_EYEBROW,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_FAINT,
        mb = space::D2,
    ));
    let h1 = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; \
         color: {c}; margin: 0; letter-spacing: -0.02em;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H1,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let p = theme::class(format!(
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
        </div>
    }
}

#[component]
pub fn UserCard(#[prop(into)] name: String, #[prop(into)] role: String) -> impl IntoView {
    let name_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let role_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));

    view! {
        <Cluster gap=Gap::Sm>
            <Avatar name=name.clone() size=AvatarSize::Md tone=format::tone_for(&name) />
            <div>
                <div class=name_cls>{name}</div>
                <div class=role_cls>{role}</div>
            </div>
        </Cluster>
    }
}
