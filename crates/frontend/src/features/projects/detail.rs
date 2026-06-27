//! Project detail: status transitions, collaborator management, group invitations, and the project's requests.

use leptos::{prelude::*, task::spawn_local};
use leptos_router::components::A;
use uuid::Uuid;

use shared::dto::group::GroupDto;
use shared::dto::ids::{GroupId, ProjectId, ProjectInviteId};
use shared::dto::project::{
    ChangeProjectStatusRequest, InviteGroupRequest, ProjectDetailDto, ProjectStatus,
};
use shared::dto::request::RequestDto;
use shared::dto::user::UserRole;

use crate::features::audit::components::{AuditTrailPanel, TrailKind};
use crate::features::groups::api as groups_api;
use crate::features::projects::api;
use crate::features::requests::api as requests_api;
use crate::features::ui;
use crate::primitives::badge::Badge;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::chart::ProgressBar;
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::Input;
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::state::auth::AuthState;
use crate::state::toast::ToastState;
use crate::theme::{self, color, space, typography};
use crate::util::format;
use crate::util::load::{self, Loadable};

#[derive(Clone, Copy)]
enum StatusTarget {
    Activate,
    Hold,
    Resume,
    Complete,
    Cancel,
}

impl StatusTarget {
    fn status(self) -> ProjectStatus {
        match self {
            Self::Activate | Self::Resume => ProjectStatus::Active,
            Self::Hold => ProjectStatus::OnHold,
            Self::Complete => ProjectStatus::Completed,
            Self::Cancel => ProjectStatus::Cancelled,
        }
    }
}

#[component]
pub fn ProjectDetail(#[prop(into)] id: Signal<Option<ProjectId>>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let auth = use_context::<AuthState>().expect("AuthState context");
    let detail: Loadable<ProjectDetailDto> = RwSignal::new(None);
    let requests: Loadable<Vec<RequestDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let invite_open = RwSignal::new(false);

    Effect::new(move |_| {
        let _ = reload.get();
        if let Some(pid) = id.get() {
            load::load(detail, api::get(pid));
            load::load(requests, requests_api::list_for_project(pid, None));
        }
    });

    let run = move |target: StatusTarget| {
        let Some(pid) = id.get_untracked() else {
            return;
        };
        let req = ChangeProjectStatusRequest {
            status: target.status(),
        };
        spawn_local(async move {
            match api::change_status(pid, &req).await {
                Ok(_) => {
                    toast.success("Project updated");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
        });
    };

    let remove_collab = move |gid: GroupId| {
        let Some(pid) = id.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match api::remove_collaborator(pid, gid).await {
                Ok(()) => {
                    toast.success("Collaborator removed");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
        });
    };

    let revoke = move |iid: ProjectInviteId| {
        spawn_local(async move {
            match api::revoke_invite(iid).await {
                Ok(_) => {
                    toast.success("Invite revoked");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
        });
    };

    let open_invite = Callback::new(move |_| invite_open.set(true));
    let invited = Callback::new(move |()| reload.update(|n| *n += 1));

    view! {
        <Stack gap=Gap::Lg>
            {ui::back_link("/projects", "Back to projects")}
            {move || match detail.get() {
                None => load::note("Loading project…"),
                Some(Err(e)) => load::load_error(&e),
                Some(Ok(d)) => {
                    let status = d.project.status;
                    let progress = d.project.progress;
                    let title_v = ui::page_title(&d.project.name);
                    let owner = d.project.owner_group.name.clone();
                    let desc_v = desc_block(&d.project.description);
                    let actions_v = status_bar(status, run);
                    let collab_v = collaborators_card(&d, remove_collab, open_invite);
                    let invites_v = pending_invites_card(&d, revoke);
                    let can_edit_progress = auth.user.with_untracked(|u| {
                        u.as_ref().is_some_and(|u| {
                            matches!(u.role, UserRole::GroupLeader | UserRole::GroupSubLeader)
                                && u.group_name.as_deref() == Some(owner.as_str())
                        })
                    });
                    let progress_editor = if can_edit_progress {
                        if let Some(pid) = id.get_untracked() {
                            view! { <ProgressEditor id=pid initial=progress reload=reload /> }.into_any()
                        } else {
                            ().into_any()
                        }
                    } else {
                        ().into_any()
                    };
                    view! {
                        <Stack gap=Gap::Lg>
                            <Card>
                                <Stack gap=Gap::Md>
                                    <Cluster gap=Gap::Sm justify="space-between".to_string()>
                                        {title_v}
                                        <Badge variant=format::project_status_variant(status)>{status.label()}</Badge>
                                    </Cluster>
                                    {ui::subtle(&format!("Owned by {owner}"))}
                                    {progress_row(progress)}
                                    {desc_v}
                                </Stack>
                            </Card>
                            {actions_v}
                            {progress_editor}
                            {collab_v}
                            {invites_v}
                            <ProjectRequests requests=requests />
                        </Stack>
                    }.into_any()
                }
            }}
            <AuditTrailPanel
                id=Signal::derive(move || id.get().map(|p| p.0))
                kind=TrailKind::Project
                refresh=reload
            />
            <InviteGroupDialog open=invite_open id=id on_invited=invited />
        </Stack>
    }
}

fn status_bar(
    status: ProjectStatus,
    run: impl Fn(StatusTarget) + Copy + Send + Sync + 'static,
) -> AnyView {
    let btn = move |label: &'static str, variant: ButtonVariant, target: StatusTarget| {
        let cb = Callback::new(move |_| run(target));
        view! { <Button variant=variant size=ButtonSize::Sm on_click=cb>{label}</Button> }
            .into_any()
    };
    let buttons: Vec<AnyView> = match status {
        ProjectStatus::Planning => vec![
            btn("Activate", ButtonVariant::Primary, StatusTarget::Activate),
            btn("Cancel", ButtonVariant::Destructive, StatusTarget::Cancel),
        ],
        ProjectStatus::Active => vec![
            btn("Put on hold", ButtonVariant::Secondary, StatusTarget::Hold),
            btn(
                "Mark complete",
                ButtonVariant::Primary,
                StatusTarget::Complete,
            ),
            btn("Cancel", ButtonVariant::Destructive, StatusTarget::Cancel),
        ],
        ProjectStatus::OnHold => vec![
            btn("Resume", ButtonVariant::Primary, StatusTarget::Resume),
            btn("Cancel", ButtonVariant::Destructive, StatusTarget::Cancel),
        ],
        ProjectStatus::Completed | ProjectStatus::Cancelled => Vec::new(),
    };
    if buttons.is_empty() {
        return ().into_any();
    }
    view! { <Card><Cluster gap=Gap::Sm>{buttons}</Cluster></Card> }.into_any()
}

fn collaborators_card(
    detail: &ProjectDetailDto,
    remove: impl Fn(GroupId) + Copy + Send + Sync + 'static,
    open_invite: Callback<leptos::ev::MouseEvent>,
) -> AnyView {
    let rows = detail
        .collaborators
        .iter()
        .map(|c| {
            let gid = c.group.id;
            let name = c.group.name.clone();
            let remove_cb = Callback::new(move |_| remove(gid));
            let row = theme::class(format!(
                "display: flex; align-items: center; gap: {g}; padding: {p} 0; border-bottom: 1px solid {b};",
                g = space::D2, p = space::D2, b = color::BORDER,
            ));
            let grow = theme::class(format!(
                "flex: 1; min-width: 0; font-family: {ff}; font-size: {fs}; color: {c};",
                ff = typography::FONT_SANS, fs = typography::TEXT_SMALL, c = color::TEXT,
            ));
            view! {
                <div class=row>
                    <Icon name=IconName::Users size=14 />
                    <span class=grow>{name}</span>
                    <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=remove_cb>"Remove"</Button>
                </div>
            }
        })
        .collect_view();
    let has = !detail.collaborators.is_empty();
    view! {
        <Card>
            <Stack gap=Gap::Md>
                <Cluster gap=Gap::Sm justify="space-between".to_string()>
                    {ui::section_heading("Collaborating groups")}
                    <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=open_invite>
                        <Icon name=IconName::Plus size=14 /> " Invite group"
                    </Button>
                </Cluster>
                {if has { view! { <div>{rows}</div> }.into_any() } else { ui::subtle("No collaborating groups yet.") }}
            </Stack>
        </Card>
    }
    .into_any()
}

fn pending_invites_card(
    detail: &ProjectDetailDto,
    revoke: impl Fn(ProjectInviteId) + Copy + Send + Sync + 'static,
) -> AnyView {
    if detail.pending_invites.is_empty() {
        return ().into_any();
    }
    let rows = detail
        .pending_invites
        .iter()
        .map(|inv| {
            let iid = inv.id;
            let name = inv.invited_group.name.clone();
            let revoke_cb = Callback::new(move |_| revoke(iid));
            let row = theme::class(format!(
                "display: flex; align-items: center; gap: {g}; padding: {p} 0; border-bottom: 1px solid {b};",
                g = space::D2, p = space::D2, b = color::BORDER,
            ));
            let grow = theme::class(format!(
                "flex: 1; min-width: 0; font-family: {ff}; font-size: {fs}; color: {c};",
                ff = typography::FONT_SANS, fs = typography::TEXT_SMALL, c = color::TEXT,
            ));
            view! {
                <div class=row>
                    <Icon name=IconName::Clock size=14 />
                    <span class=grow>{name}</span>
                    <Badge>"Pending"</Badge>
                    <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=revoke_cb>"Revoke"</Button>
                </div>
            }
        })
        .collect_view();
    view! { <Card><Stack gap=Gap::Md>{ui::section_heading("Pending invites")}<div>{rows}</div></Stack></Card> }
        .into_any()
}

#[component]
fn ProjectRequests(requests: Loadable<Vec<RequestDto>>) -> impl IntoView {
    view! {
        <Card>
            <Stack gap=Gap::Md>
                {ui::section_heading("Requests")}
                {move || match requests.get() {
                    None => load::note("Loading…"),
                    Some(Err(e)) => load::load_error(&e),
                    Some(Ok(list)) if list.is_empty() => ui::subtle("No requests against this project yet."),
                    Some(Ok(list)) => {
                        let rows = list.into_iter().map(|r| {
                            let href = format!("/requests/{}", r.id.0);
                            let title = r.title.clone();
                            let status = r.status;
                            let row = theme::class(format!(
                                "display: flex; align-items: center; gap: {g}; padding: {p} 0; border-bottom: 1px solid {b};",
                                g = space::D2, p = space::D2, b = color::BORDER,
                            ));
                            let link = theme::class(format!(
                                "flex: 1; min-width: 0; color: {c}; text-decoration: none; font-family: {ff}; \
                                 font-size: {fs}; &:hover {{ color: {a}; }}",
                                c = color::TEXT, ff = typography::FONT_SANS, fs = typography::TEXT_SMALL, a = color::ACCENT,
                            ));
                            view! {
                                <div class=row>
                                    <A href=href attr:class=link>{title}</A>
                                    <Badge variant=crate::util::format::request_status_variant(status)>{status.label()}</Badge>
                                </div>
                            }
                        }).collect_view();
                        view! { <div>{rows}</div> }.into_any()
                    }
                }}
            </Stack>
        </Card>
    }
}

#[component]
fn InviteGroupDialog(
    open: RwSignal<bool>,
    #[prop(into)] id: Signal<Option<ProjectId>>,
    on_invited: Callback<()>,
) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let groups: Loadable<Vec<GroupDto>> = RwSignal::new(None);
    load::load(groups, groups_api::list());
    let group = RwSignal::new(None::<GroupId>);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_group = Callback::new(move |s: String| group.set(Uuid::parse_str(&s).ok().map(GroupId)));
    let group_value =
        Signal::derive(move || group.get().map(|g| g.0.to_string()).unwrap_or_default());

    let confirm = Callback::new(move |_| {
        let Some(gid) = group.get_untracked() else {
            toast.error("Pick a group to invite.");
            return;
        };
        let Some(pid) = id.get_untracked() else {
            return;
        };
        open.set(false);
        let req = InviteGroupRequest { group_id: gid };
        spawn_local(async move {
            match api::invite_group(pid, &req).await {
                Ok(_) => {
                    toast.success("Group invited");
                    on_invited.run(());
                }
                Err(e) => toast.error_from(&e),
            }
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Invite a group" subtitle="Cross-group access is granted at the group level." />
            <DialogBody>
                <Select value=group_value on_change=on_group>
                    <option value="">"Select a group…"</option>
                    {move || groups.get().and_then(Result::ok).map(|l| {
                        l.into_iter().map(|g| {
                            let id = g.id.0.to_string();
                            view! { <option value=id>{g.name}</option> }
                        }).collect_view()
                    })}
                </Select>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=confirm>"Send invite"</Button>
            </DialogFooter>
        </Dialog>
    }
}

fn progress_row(progress: u8) -> AnyView {
    let wrap = theme::class("display: flex; align-items: center; gap: 12px;");
    let label = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; white-space: nowrap;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    let bar = theme::class("flex: 1; max-width: 260px;");
    view! {
        <div class=wrap>
            <span class=label>{format!("Progress {progress}%")}</span>
            <div class=bar>
                <ProgressBar value=Signal::derive(move || progress) />
            </div>
        </div>
    }
    .into_any()
}

#[component]
fn ProgressEditor(id: ProjectId, initial: u8, reload: RwSignal<u32>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let value = RwSignal::new(initial.to_string());
    let saving = RwSignal::new(false);
    let on_input = Callback::new(move |v: String| value.set(v));
    let save = Callback::new(move |_| {
        if saving.get_untracked() {
            return;
        }
        let parsed = value.get_untracked().trim().parse::<u8>();
        let Ok(p) = parsed else {
            toast.error("Progress must be a whole number between 0 and 100.");
            return;
        };
        if p > 100 {
            toast.error("Progress must be between 0 and 100.");
            return;
        }
        saving.set(true);
        spawn_local(async move {
            match api::set_progress(id, p).await {
                Ok(_) => {
                    toast.success("Progress updated");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
            saving.set(false);
        });
    });
    let input_wrap = theme::class("width: 110px;");
    view! {
        <Card>
            <Stack gap=Gap::Sm>
                {ui::section_heading("Set progress")}
                <Cluster gap=Gap::Sm>
                    <div class=input_wrap>
                        <Input value=value on_input=on_input placeholder="0-100" />
                    </div>
                    <Button
                        variant=ButtonVariant::Primary
                        size=ButtonSize::Sm
                        on_click=save
                        disabled=saving.get()
                    >
                        "Save"
                    </Button>
                </Cluster>
            </Stack>
        </Card>
    }
}

fn desc_block(description: &str) -> AnyView {
    let cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; line-height: 1.55; white-space: pre-wrap;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    ));
    view! { <p class=cls>{description.to_owned()}</p> }.into_any()
}
