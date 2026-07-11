//! Group detail: the org-tree roster (leader, sub-leaders, members) plus member administration: add, change role, remove, transfer leadership.

use leptos::{ev::MouseEvent, prelude::*, task};

use shared::dto::group::{
    AddMemberRequest, ChangeMemberRoleRequest, GroupDetailDto, GroupKind, GroupRole, MembershipDto,
};
use shared::dto::ids::{GroupId, UserId};

use crate::features::groups::api;
use crate::features::ui;
use crate::features::users::picker::UserPicker;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::badge::{Badge, BadgeVariant};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::FieldLabel;
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::state::toast::ToastState;
use crate::theme::{self, color, space, typography};
use crate::util::format;
use crate::util::load::{self, Loadable};

#[component]
pub fn GroupDetail(#[prop(into)] id: Signal<Option<GroupId>>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let detail: Loadable<GroupDetailDto> = Loadable::new();
    let reload = RwSignal::new(0u32);
    let add_open = RwSignal::new(false);
    let transfer_open = RwSignal::new(false);

    Effect::new(move |_| {
        let _ = reload.get();
        if let Some(gid) = id.get() {
            load::load(detail, api::get(gid));
        }
    });

    let do_change_role = move |uid: UserId, role: GroupRole| {
        let Some(gid) = id.get_untracked() else {
            return;
        };
        let req = ChangeMemberRoleRequest { role };
        task::spawn_local(async move {
            match api::change_role(gid, uid, &req).await {
                Ok(_) => {
                    toast.success("Role updated");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
        });
    };
    let do_remove = move |uid: UserId| {
        let Some(gid) = id.get_untracked() else {
            return;
        };
        task::spawn_local(async move {
            match api::remove_member(gid, uid).await {
                Ok(()) => {
                    toast.success("Member removed");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
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
            d.members
                .iter()
                .find(|m| m.role == GroupRole::Leader)
                .map(|m| m.user.id)
        })
    });

    view! {
        <Stack gap=Gap::Lg>
            {ui::back_link("/groups", "Back to groups")}
            {move || match detail.get() {
                None => load::note("Loading group…"),
                Some(Err(e)) => load::load_error(&e),
                Some(Ok(d)) => {
                    let title_v = ui::page_title(&d.group.name);
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
                                    {ui::subtle(&format!("{count} members"))}
                                    {if desc.is_empty() { ().into_any() } else {
                                        let cls = theme::class(format!(
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
    open_add: Callback<MouseEvent>,
    open_transfer: Callback<MouseEvent>,
) -> AnyView {
    let rows = detail
        .members
        .iter()
        .map(|m| member_row(m, change_role, remove))
        .collect_view();
    view! {
        <Card>
            <Stack gap=Gap::Md>
                <Cluster gap=Gap::Sm justify="space-between".to_string()>
                    {ui::section_heading("Members")}
                    <Cluster gap=Gap::Xs>
                        <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=open_transfer>
                            <Icon name=IconName::Crown size=14 /> "Transfer lead"
                        </Button>
                        <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=open_add>
                            <Icon name=IconName::Plus size=14 /> "Add member"
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
    let row = theme::class(format!(
        "display: flex; align-items: center; gap: {g}; padding: {p} 0; border-bottom: 1px solid {b};",
        g = space::D3,
        p = space::D2,
        b = color::BORDER,
    ));
    let grow = theme::class(format!(
        "flex: 1; min-width: 0; font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_MEDIUM,
        c = color::TEXT,
    ));
    let select_wrap = theme::class("width: 130px;");
    let remove_cb = Callback::new(move |_| remove(uid));

    let controls = if role == GroupRole::Leader {
        view! { <Badge variant=BadgeVariant::Accent><Icon name=IconName::Crown size=10 /> "Leader"</Badge> }
            .into_any()
    } else {
        let on_role = Callback::new(move |v: String| {
            change_role(uid, GroupRole::from_wire(&v).unwrap_or(GroupRole::Member));
        });
        let role_value = Signal::derive(move || role.as_str().to_owned());
        view! {
            <div class=select_wrap.clone()>
                <Select value=role_value on_change=on_role>
                    <option value=GroupRole::SubLeader.as_str()>"Sub-leader"</option>
                    <option value=GroupRole::Member.as_str()>"Member"</option>
                </Select>
            </div>
            <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=remove_cb>"Remove"</Button>
        }
        .into_any()
    };

    view! {
        <div class=row>
            <Avatar name=name.clone() size=AvatarSize::Sm tone=format::tone_for(&name) />
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
    let submitting = RwSignal::new(false);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_select = Callback::new(move |u: UserId| target.set(Some(u)));
    let on_role = Callback::new(move |v: String| {
        role.set(GroupRole::from_wire(&v).unwrap_or(GroupRole::Member));
    });
    let role_value = Signal::derive(move || role.get().as_str().to_owned());

    let confirm = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        let Some(uid) = target.get_untracked() else {
            toast.error("Pick a person to add.");
            return;
        };
        let Some(gid) = id.get_untracked() else {
            return;
        };
        let req = AddMemberRequest {
            user_id: uid,
            role: role.get_untracked(),
        };
        submitting.set(true);
        task::spawn_local(async move {
            match api::add_member(gid, &req).await {
                Ok(_) => {
                    toast.success("Member added");
                    target.set(None);
                    open.set(false);
                    on_added.run(());
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
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
                            <option value=GroupRole::Member.as_str()>"Member"</option>
                            <option value=GroupRole::SubLeader.as_str()>"Sub-leader"</option>
                        </Select>
                    </div>
                </Stack>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=confirm disabled=submitting>"Add member"</Button>
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
    let submitting = RwSignal::new(false);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_select = Callback::new(move |u: UserId| target.set(Some(u)));

    let confirm = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        let Some(to) = target.get_untracked() else {
            toast.error("Pick the new leader.");
            return;
        };
        let Some(from) = from.get_untracked() else {
            toast.error("This group has no current leader to transfer from.");
            return;
        };
        let Some(gid) = id.get_untracked() else {
            return;
        };
        submitting.set(true);
        task::spawn_local(async move {
            match api::transfer_leadership(gid, from, to).await {
                Ok(()) => {
                    toast.success("Leadership transferred");
                    target.set(None);
                    open.set(false);
                    on_transferred.run(());
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
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
                <Button variant=ButtonVariant::Primary on_click=confirm disabled=submitting>"Transfer"</Button>
            </DialogFooter>
        </Dialog>
    }
}
