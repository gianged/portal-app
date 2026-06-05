//! Group UI: the org directory with a create dialog, and the detail page with an
//! org-tree roster (leader → sub-leaders → members) plus member administration —
//! add, change role, remove, transfer leadership, and edit metadata.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use shared::dto::group::{
    AddMemberRequest, ChangeMemberRoleRequest, CreateGroupRequest, GroupDetailDto, GroupDto,
    GroupKind, GroupRole, MembershipDto,
};
use shared::dto::ids::{GroupId, UserId};
use shared::validation::group::{validate_group_description, validate_group_name};

use crate::features::groups::api;
use crate::features::ui::{back_link, page_title, section_heading, subtle};
use crate::features::users::components::UserPicker;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::badge::{Badge, BadgeVariant};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::{FieldError, FieldLabel, Input};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::textarea::Textarea;
use crate::state::toast::ToastState;
use crate::theme::{class, color, space, typography};
use crate::util::format::tone_for;
use crate::util::load::{Loadable, load, note};

fn role_wire(r: GroupRole) -> &'static str {
    match r {
        GroupRole::Leader => "leader",
        GroupRole::SubLeader => "sub_leader",
        GroupRole::Member => "member",
    }
}

fn role_from_wire(s: &str) -> GroupRole {
    match s {
        "leader" => GroupRole::Leader,
        "sub_leader" => GroupRole::SubLeader,
        _ => GroupRole::Member,
    }
}

fn kind_wire(k: GroupKind) -> &'static str {
    match k {
        GroupKind::Standard => "standard",
        GroupKind::It => "it",
    }
}

// ─────────────────────────── Index ───────────────────────────

#[component]
pub fn GroupsIndex() -> impl IntoView {
    let groups: Loadable<Vec<GroupDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let create_open = RwSignal::new(false);

    Effect::new(move |_| {
        let _ = reload.get();
        load(groups, api::list());
    });

    let open_create = Callback::new(move |_| create_open.set(true));
    let created = Callback::new(move |()| reload.update(|n| *n += 1));

    view! {
        <Stack gap=Gap::Lg>
            <Cluster gap=Gap::Sm justify="space-between".to_string()>
                {section_heading("Groups")}
                <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=open_create>
                    <Icon name=IconName::Plus size=14 /> " New group"
                </Button>
            </Cluster>
            {move || match groups.get() {
                None => note("Loading groups…", false),
                Some(Err(e)) => note(&format!("Couldn't load groups: {e}"), true),
                Some(Ok(list)) if list.is_empty() => view! {
                    <EmptyState icon=IconName::Users title="No groups yet" description="Create the first group to get started." />
                }.into_any(),
                Some(Ok(list)) => {
                    let grid = class(format!(
                        "display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: {g};",
                        g = space::D4,
                    ));
                    view! { <div class=grid>{list.into_iter().map(group_card).collect_view()}</div> }.into_any()
                }
            }}
            <CreateGroupDialog open=create_open on_created=created />
        </Stack>
    }
}

fn group_card(g: GroupDto) -> impl IntoView {
    let href = format!("/groups/{}", g.id.0);
    let name = g.name.clone();
    let desc = g.description.clone();
    let count = g.member_count;
    let kind = g.kind;
    let link = class("text-decoration: none; display: block;");
    let name_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H3,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let desc_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; margin: 0; \
         display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));
    view! {
        <A href=href attr:class=link>
            <Card>
                <Stack gap=Gap::Sm>
                    <Cluster gap=Gap::Sm justify="space-between".to_string()>
                        <h3 class=name_cls>{name}</h3>
                        {match kind {
                            GroupKind::It => view! { <Badge variant=BadgeVariant::Accent>"IT"</Badge> }.into_any(),
                            GroupKind::Standard => ().into_any(),
                        }}
                    </Cluster>
                    <p class=desc_cls>{desc}</p>
                    {subtle(&format!("{count} members"))}
                </Stack>
            </Card>
        </A>
    }
}

#[component]
fn CreateGroupDialog(open: RwSignal<bool>, on_created: Callback<()>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let kind = RwSignal::new(GroupKind::Standard);
    let name_err = RwSignal::new(None::<String>);
    let desc_err = RwSignal::new(None::<String>);
    let submitting = RwSignal::new(false);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_kind = Callback::new(move |v: String| {
        kind.set(if v == "it" { GroupKind::It } else { GroupKind::Standard });
    });
    let kind_value = Signal::derive(move || kind_wire(kind.get()).to_owned());

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        name_err.set(None);
        desc_err.set(None);
        let n = name.get_untracked();
        let d = description.get_untracked();
        let mut ok = true;
        if let Err(e) = validate_group_name(&n) {
            name_err.set(Some(e.to_string()));
            ok = false;
        }
        if let Err(e) = validate_group_description(&d) {
            desc_err.set(Some(e.to_string()));
            ok = false;
        }
        if !ok {
            return;
        }
        submitting.set(true);
        let req = CreateGroupRequest { name: n, description: d, kind: kind.get_untracked() };
        spawn_local(async move {
            match api::create(&req).await {
                Ok(_) => {
                    toast.success("Group created");
                    name.set(String::new());
                    description.set(String::new());
                    open.set(false);
                    on_created.run(());
                }
                Err(e) => toast.error(e.to_string()),
            }
            submitting.set(false);
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="New group" subtitle="Create a team or the IT group." />
            <DialogBody>
                <Stack gap=Gap::Lg>
                    <div>
                        <FieldLabel for_id="gr-name">"Name"</FieldLabel>
                        <Input value=name on_input=Callback::new(move |v| name.set(v)) placeholder="e.g. Platform Engineering" />
                        {move || name_err.get().map(|m| view! { <FieldError message=m /> })}
                    </div>
                    <div>
                        <FieldLabel for_id="gr-desc">"Description"</FieldLabel>
                        <Textarea value=description on_input=Callback::new(move |v| description.set(v)) placeholder="What does this group own?" />
                        {move || desc_err.get().map(|m| view! { <FieldError message=m /> })}
                    </div>
                    <div>
                        <FieldLabel for_id="gr-kind">"Kind"</FieldLabel>
                        <Select value=kind_value on_change=on_kind>
                            <option value="standard">"Standard"</option>
                            <option value="it">"IT"</option>
                        </Select>
                    </div>
                </Stack>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=submit disabled=submitting.get()>
                    {move || if submitting.get() { "Creating…" } else { "Create group" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}

// ─────────────────────────── Detail ───────────────────────────

#[component]
pub fn GroupDetail(#[prop(into)] id: Signal<Option<GroupId>>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let detail: Loadable<GroupDetailDto> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let add_open = RwSignal::new(false);
    let transfer_open = RwSignal::new(false);

    Effect::new(move |_| {
        let _ = reload.get();
        if let Some(gid) = id.get() {
            load(detail, api::get(gid));
        }
    });

    let do_change_role = move |uid: UserId, role: GroupRole| {
        let Some(gid) = id.get_untracked() else { return };
        let req = ChangeMemberRoleRequest { role };
        spawn_local(async move {
            match api::change_role(gid, uid, &req).await {
                Ok(_) => {
                    toast.success("Role updated");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error(e.to_string()),
            }
        });
    };
    let do_remove = move |uid: UserId| {
        let Some(gid) = id.get_untracked() else { return };
        spawn_local(async move {
            match api::remove_member(gid, uid).await {
                Ok(()) => {
                    toast.success("Member removed");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error(e.to_string()),
            }
        });
    };

    let open_add = Callback::new(move |_| add_open.set(true));
    let open_transfer = Callback::new(move |_| transfer_open.set(true));
    let added = Callback::new(move |()| reload.update(|n| *n += 1));
    let transferred = Callback::new(move |()| reload.update(|n| *n += 1));

    // Current leader id, for the transfer-leadership "from".
    let current_leader = Signal::derive(move || {
        detail.get().and_then(Result::ok).and_then(|d| {
            d.members.iter().find(|m| m.role == GroupRole::Leader).map(|m| m.user.id)
        })
    });

    view! {
        <Stack gap=Gap::Lg>
            {back_link("/groups", "Back to groups")}
            {move || match detail.get() {
                None => note("Loading group…", false),
                Some(Err(e)) => note(&format!("Couldn't load group: {e}"), true),
                Some(Ok(d)) => {
                    let title_v = page_title(&d.group.name);
                    let kind = d.group.kind;
                    let count = d.group.member_count;
                    let desc = d.group.description.clone();
                    let roster_v = roster_card(&d, do_change_role, do_remove, open_add, open_transfer);
                    view! {
                        <Stack gap=Gap::Lg>
                            <Card>
                                <Stack gap=Gap::Sm>
                                    <Cluster gap=Gap::Sm justify="space-between".to_string()>
                                        {title_v}
                                        {match kind {
                                            GroupKind::It => view! { <Badge variant=BadgeVariant::Accent>"IT"</Badge> }.into_any(),
                                            GroupKind::Standard => ().into_any(),
                                        }}
                                    </Cluster>
                                    {subtle(&format!("{count} members"))}
                                    {if desc.is_empty() { ().into_any() } else {
                                        let cls = class(format!(
                                            "font-family: {ff}; font-size: {fs}; color: {c};",
                                            ff = typography::FONT_SANS, fs = typography::TEXT_SMALL, c = color::TEXT,
                                        ));
                                        view! { <p class=cls>{desc}</p> }.into_any()
                                    }}
                                </Stack>
                            </Card>
                            {roster_v}
                        </Stack>
                    }.into_any()
                }
            }}
            <AddMemberDialog open=add_open id=id on_added=added />
            <TransferDialog open=transfer_open id=id from=current_leader on_transferred=transferred />
        </Stack>
    }
}

fn roster_card(
    detail: &GroupDetailDto,
    change_role: impl Fn(UserId, GroupRole) + Copy + Send + Sync + 'static,
    remove: impl Fn(UserId) + Copy + Send + Sync + 'static,
    open_add: Callback<leptos::ev::MouseEvent>,
    open_transfer: Callback<leptos::ev::MouseEvent>,
) -> AnyView {
    let mut rows: Vec<AnyView> = Vec::new();
    for m in &detail.members {
        rows.push(member_row(m, change_role, remove));
    }
    view! {
        <Card>
            <Stack gap=Gap::Md>
                <Cluster gap=Gap::Sm justify="space-between".to_string()>
                    {section_heading("Members")}
                    <Cluster gap=Gap::Xs>
                        <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=open_transfer>
                            <Icon name=IconName::Crown size=14 /> " Transfer lead"
                        </Button>
                        <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=open_add>
                            <Icon name=IconName::Plus size=14 /> " Add member"
                        </Button>
                    </Cluster>
                </Cluster>
                <div>{rows}</div>
            </Stack>
        </Card>
    }
    .into_any()
}

fn member_row(
    m: &MembershipDto,
    change_role: impl Fn(UserId, GroupRole) + Copy + Send + Sync + 'static,
    remove: impl Fn(UserId) + Copy + Send + Sync + 'static,
) -> AnyView {
    let uid = m.user.id;
    let name = m.user.full_name.clone();
    let role = m.role;
    let row = class(format!(
        "display: flex; align-items: center; gap: {g}; padding: {p} 0; border-bottom: 1px solid {b};",
        g = space::D3,
        p = space::D2,
        b = color::BORDER,
    ));
    let grow = class(format!(
        "flex: 1; min-width: 0; font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_MEDIUM,
        c = color::TEXT,
    ));
    let select_wrap = class("width: 130px;");
    let remove_cb = Callback::new(move |_| remove(uid));

    let controls = if role == GroupRole::Leader {
        view! { <Badge variant=BadgeVariant::Accent><Icon name=IconName::Crown size=10 /> " Leader"</Badge> }
            .into_any()
    } else {
        let on_role = Callback::new(move |v: String| change_role(uid, role_from_wire(&v)));
        let role_value = Signal::derive(move || role_wire(role).to_owned());
        view! {
            <div class=select_wrap.clone()>
                <Select value=role_value on_change=on_role>
                    <option value="sub_leader">"Sub-leader"</option>
                    <option value="member">"Member"</option>
                </Select>
            </div>
            <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=remove_cb>"Remove"</Button>
        }
        .into_any()
    };

    view! {
        <div class=row>
            <Avatar name=name.clone() size=AvatarSize::Sm tone=tone_for(&name) />
            <span class=grow>{name}</span>
            {controls}
        </div>
    }
    .into_any()
}

#[component]
fn AddMemberDialog(
    open: RwSignal<bool>,
    #[prop(into)] id: Signal<Option<GroupId>>,
    on_added: Callback<()>,
) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let target = RwSignal::new(None::<UserId>);
    let role = RwSignal::new(GroupRole::Member);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_select = Callback::new(move |u: UserId| target.set(Some(u)));
    let on_role = Callback::new(move |v: String| role.set(role_from_wire(&v)));
    let role_value = Signal::derive(move || role_wire(role.get()).to_owned());

    let confirm = Callback::new(move |_| {
        let Some(uid) = target.get_untracked() else {
            toast.error("Pick a person to add.");
            return;
        };
        let Some(gid) = id.get_untracked() else { return };
        open.set(false);
        let req = AddMemberRequest { user_id: uid, role: role.get_untracked() };
        spawn_local(async move {
            match api::add_member(gid, &req).await {
                Ok(_) => {
                    toast.success("Member added");
                    target.set(None);
                    on_added.run(());
                }
                Err(e) => toast.error(e.to_string()),
            }
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Add member" subtitle="Add a person to this group." />
            <DialogBody>
                <Stack gap=Gap::Lg>
                    <div>
                        <FieldLabel for_id="add-user">"Person"</FieldLabel>
                        <UserPicker selected=target on_select=on_select />
                    </div>
                    <div>
                        <FieldLabel for_id="add-role">"Role"</FieldLabel>
                        <Select value=role_value on_change=on_role>
                            <option value="member">"Member"</option>
                            <option value="sub_leader">"Sub-leader"</option>
                        </Select>
                    </div>
                </Stack>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=confirm>"Add member"</Button>
            </DialogFooter>
        </Dialog>
    }
}

#[component]
fn TransferDialog(
    open: RwSignal<bool>,
    #[prop(into)] id: Signal<Option<GroupId>>,
    #[prop(into)] from: Signal<Option<UserId>>,
    on_transferred: Callback<()>,
) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let target = RwSignal::new(None::<UserId>);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_select = Callback::new(move |u: UserId| target.set(Some(u)));

    let confirm = Callback::new(move |_| {
        let Some(to) = target.get_untracked() else {
            toast.error("Pick the new leader.");
            return;
        };
        let Some(from) = from.get_untracked() else {
            toast.error("This group has no current leader to transfer from.");
            return;
        };
        let Some(gid) = id.get_untracked() else { return };
        open.set(false);
        spawn_local(async move {
            match api::transfer_leadership(gid, from, to).await {
                Ok(()) => {
                    toast.success("Leadership transferred");
                    target.set(None);
                    on_transferred.run(());
                }
                Err(e) => toast.error(e.to_string()),
            }
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Transfer leadership" subtitle="The current leader becomes a sub-leader." />
            <DialogBody>
                <UserPicker selected=target on_select=on_select />
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=confirm>"Transfer"</Button>
            </DialogFooter>
        </Dialog>
    }
}
