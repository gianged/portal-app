//! Project UI: a per-group index (owner-group selector → project cards + incoming
//! invites) and a detail view with status transitions, collaborator management,
//! group invitations, and the project's requests.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use uuid::Uuid;

use shared::dto::group::GroupDto;
use shared::dto::ids::{GroupId, ProjectId, ProjectInviteId};
use shared::dto::project::{
    ChangeProjectStatusRequest, CreateProjectRequest, InviteGroupRequest, ProjectDetailDto,
    ProjectDto, ProjectInviteDto, ProjectStatus, RespondInviteRequest,
};
use shared::dto::request::RequestDto;
use shared::validation::project::{validate_project_description, validate_project_name};

use crate::features::groups::api as groups_api;
use crate::features::projects::api;
use crate::features::requests::api as requests_api;
use crate::features::ui::{back_link, page_title, section_heading, subtle};
use crate::primitives::badge::Badge;
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
use crate::state::auth::AuthState;
use crate::state::toast::ToastState;
use crate::theme::{class, color, space, typography};
use crate::util::format::project_status_variant;
use crate::util::load::{Loadable, load, load_error, note};

// ─────────────────────────── Index ───────────────────────────

#[component]
pub fn ProjectsIndex() -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let groups: Loadable<Vec<GroupDto>> = RwSignal::new(None);
    load(groups, groups_api::list());
    let group = RwSignal::new(None::<GroupId>);
    let projects: Loadable<Vec<ProjectDto>> = RwSignal::new(None);
    let invites: Loadable<Vec<ProjectInviteDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let create_open = RwSignal::new(false);

    // Auto-select the caller's own group once the directory loads.
    Effect::new(move |_| {
        if group.get_untracked().is_some() {
            return;
        }
        if let Some(Ok(list)) = groups.get() {
            let my = auth
                .user
                .with_untracked(|u| u.as_ref().and_then(|x| x.group_name.clone()));
            if let Some(name) = my
                && let Some(g) = list.iter().find(|g| g.name == name)
            {
                group.set(Some(g.id));
            }
        }
    });

    Effect::new(move |_| {
        let _ = reload.get();
        if let Some(g) = group.get() {
            load(projects, api::list_for_owner_group(g));
            load(invites, api::list_invites_for_group(g));
        }
    });

    let on_group = Callback::new(move |s: String| group.set(Uuid::parse_str(&s).ok().map(GroupId)));
    let group_value =
        Signal::derive(move || group.get().map(|g| g.0.to_string()).unwrap_or_default());
    let open_create = Callback::new(move |_| create_open.set(true));
    let created = Callback::new(move |()| reload.update(|n| *n += 1));
    let responded = Callback::new(move |()| reload.update(|n| *n += 1));
    let select_wrap = class("width: 260px;");

    view! {
        <Stack gap=Gap::Lg>
            <Cluster gap=Gap::Sm justify="space-between".to_string()>
                <div class=select_wrap>
                    <Select value=group_value on_change=on_group>
                        <option value="">"Select a group…"</option>
                        {move || groups.get().and_then(Result::ok).map(|l| {
                            l.into_iter().map(|g| {
                                let id = g.id.0.to_string();
                                view! { <option value=id>{g.name}</option> }
                            }).collect_view()
                        })}
                    </Select>
                </div>
                <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=open_create>
                    <Icon name=IconName::Plus size=14 /> " New project"
                </Button>
            </Cluster>

            <InvitesInbox invites=invites on_responded=responded />

            {move || {
                if group.get().is_none() {
                    return view! {
                        <EmptyState
                            icon=IconName::Folder
                            title="Pick a group"
                            description="Choose a group above to see the projects it owns."
                        />
                    }.into_any();
                }
                match projects.get() {
                    None => note("Loading projects…"),
                    Some(Err(e)) => load_error(&e),
                    Some(Ok(list)) if list.is_empty() => view! {
                        <EmptyState
                            icon=IconName::Folder
                            title="No projects yet"
                            description="This group doesn't own any projects."
                        />
                    }.into_any(),
                    Some(Ok(list)) => {
                        let grid = class(format!(
                            "display: grid; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); gap: {g};",
                            g = space::D4,
                        ));
                        view! { <div class=grid>{list.into_iter().map(project_card).collect_view()}</div> }.into_any()
                    }
                }
            }}
            <CreateProjectDialog open=create_open owner_group=group on_created=created />
        </Stack>
    }
}

fn project_card(p: ProjectDto) -> impl IntoView {
    let href = format!("/projects/{}", p.id.0);
    let name = p.name.clone();
    let desc = p.description.clone();
    let status = p.status;
    let owner = p.owner_group.name.clone();
    let card_link = class("text-decoration: none; display: block;");
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
        <A href=href attr:class=card_link>
            <Card>
                <Stack gap=Gap::Sm>
                    <Cluster gap=Gap::Sm justify="space-between".to_string()>
                        <h3 class=name_cls>{name}</h3>
                        <Badge variant=project_status_variant(status)>{status.label()}</Badge>
                    </Cluster>
                    <p class=desc_cls>{desc}</p>
                    {subtle(&format!("Owned by {owner}"))}
                </Stack>
            </Card>
        </A>
    }
}

#[component]
fn InvitesInbox(
    invites: Loadable<Vec<ProjectInviteDto>>,
    on_responded: Callback<()>,
) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let respond = move |invite: ProjectInviteId, accept: bool| {
        spawn_local(async move {
            let req = RespondInviteRequest { accept };
            match api::respond_invite(invite, &req).await {
                Ok(_) => {
                    toast.success(if accept {
                        "Invite accepted"
                    } else {
                        "Invite declined"
                    });
                    on_responded.run(());
                }
                Err(e) => toast.error_from(&e),
            }
        });
    };

    view! {
        {move || match invites.get() {
            Some(Ok(list)) if !list.is_empty() => {
                let rows = list.into_iter().map(|inv| {
                    let invited_by = inv.invited_by.full_name.clone();
                    let iid = inv.id;
                    let accept = Callback::new(move |_| respond(iid, true));
                    let decline = Callback::new(move |_| respond(iid, false));
                    let row = class(format!(
                        "display: flex; align-items: center; gap: {g}; padding: {p} 0; \
                         border-bottom: 1px solid {b};",
                        g = space::D2, p = space::D2, b = color::BORDER,
                    ));
                    let grow = class("flex: 1; min-width: 0;");
                    view! {
                        <div class=row>
                            <Icon name=IconName::Folder size=14 />
                            <span class=grow>{subtle(&format!("Invited by {invited_by}"))}</span>
                            <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=accept>"Accept"</Button>
                            <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=decline>"Decline"</Button>
                        </div>
                    }
                }).collect_view();
                view! { <Card><Stack gap=Gap::Sm>{section_heading("Incoming invites")}{rows}</Stack></Card> }.into_any()
            }
            _ => ().into_any(),
        }}
    }
}

#[component]
fn CreateProjectDialog(
    open: RwSignal<bool>,
    owner_group: RwSignal<Option<GroupId>>,
    on_created: Callback<()>,
) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let name = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let name_err = RwSignal::new(None::<String>);
    let desc_err = RwSignal::new(None::<String>);
    let submitting = RwSignal::new(false);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        let Some(gid) = owner_group.get_untracked() else {
            toast.error("Select an owner group first.");
            return;
        };
        name_err.set(None);
        desc_err.set(None);
        let n = name.get_untracked();
        let d = description.get_untracked();
        let mut ok = true;
        if let Err(e) = validate_project_name(&n) {
            name_err.set(Some(e.to_string()));
            ok = false;
        }
        if let Err(e) = validate_project_description(&d) {
            desc_err.set(Some(e.to_string()));
            ok = false;
        }
        if !ok {
            return;
        }
        submitting.set(true);
        let req = CreateProjectRequest {
            owner_group_id: gid,
            name: n,
            description: d,
        };
        spawn_local(async move {
            match api::create(&req).await {
                Ok(_) => {
                    toast.success("Project created");
                    name.set(String::new());
                    description.set(String::new());
                    open.set(false);
                    on_created.run(());
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="New project" subtitle="Owned by the selected group." />
            <DialogBody>
                <Stack gap=Gap::Lg>
                    <div>
                        <FieldLabel for_id="pr-name">"Name"</FieldLabel>
                        <Input value=name on_input=Callback::new(move |v| name.set(v)) placeholder="e.g. Helios" />
                        {move || name_err.get().map(|m| view! { <FieldError message=m /> })}
                    </div>
                    <div>
                        <FieldLabel for_id="pr-desc">"Description"</FieldLabel>
                        <Textarea value=description on_input=Callback::new(move |v| description.set(v)) placeholder="What does this project deliver?" />
                        {move || desc_err.get().map(|m| view! { <FieldError message=m /> })}
                    </div>
                </Stack>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=submit disabled=submitting.get()>
                    {move || if submitting.get() { "Creating…" } else { "Create project" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}

// ─────────────────────────── Detail ───────────────────────────

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
    let detail: Loadable<ProjectDetailDto> = RwSignal::new(None);
    let requests: Loadable<Vec<RequestDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let invite_open = RwSignal::new(false);

    Effect::new(move |_| {
        let _ = reload.get();
        if let Some(pid) = id.get() {
            load(detail, api::get(pid));
            load(requests, requests_api::list_for_project(pid, None));
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
            {back_link("/projects", "Back to projects")}
            {move || match detail.get() {
                None => note("Loading project…"),
                Some(Err(e)) => load_error(&e),
                Some(Ok(d)) => {
                    let status = d.project.status;
                    let title_v = page_title(&d.project.name);
                    let owner = d.project.owner_group.name.clone();
                    let desc_v = desc_block(&d.project.description);
                    let actions_v = status_bar(status, run);
                    let collab_v = collaborators_card(&d, remove_collab, open_invite);
                    let invites_v = pending_invites_card(&d, revoke);
                    view! {
                        <Stack gap=Gap::Lg>
                            <Card>
                                <Stack gap=Gap::Md>
                                    <Cluster gap=Gap::Sm justify="space-between".to_string()>
                                        {title_v}
                                        <Badge variant=project_status_variant(status)>{status.label()}</Badge>
                                    </Cluster>
                                    {subtle(&format!("Owned by {owner}"))}
                                    {desc_v}
                                </Stack>
                            </Card>
                            {actions_v}
                            {collab_v}
                            {invites_v}
                            <ProjectRequests requests=requests />
                        </Stack>
                    }.into_any()
                }
            }}
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
            let row = class(format!(
                "display: flex; align-items: center; gap: {g}; padding: {p} 0; border-bottom: 1px solid {b};",
                g = space::D2, p = space::D2, b = color::BORDER,
            ));
            let grow = class(format!(
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
                    {section_heading("Collaborating groups")}
                    <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=open_invite>
                        <Icon name=IconName::Plus size=14 /> " Invite group"
                    </Button>
                </Cluster>
                {if has { view! { <div>{rows}</div> }.into_any() } else { subtle("No collaborating groups yet.") }}
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
            let row = class(format!(
                "display: flex; align-items: center; gap: {g}; padding: {p} 0; border-bottom: 1px solid {b};",
                g = space::D2, p = space::D2, b = color::BORDER,
            ));
            let grow = class(format!(
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
    view! { <Card><Stack gap=Gap::Md>{section_heading("Pending invites")}<div>{rows}</div></Stack></Card> }
        .into_any()
}

#[component]
fn ProjectRequests(requests: Loadable<Vec<RequestDto>>) -> impl IntoView {
    view! {
        <Card>
            <Stack gap=Gap::Md>
                {section_heading("Requests")}
                {move || match requests.get() {
                    None => note("Loading…"),
                    Some(Err(e)) => load_error(&e),
                    Some(Ok(list)) if list.is_empty() => subtle("No requests against this project yet."),
                    Some(Ok(list)) => {
                        let rows = list.into_iter().map(|r| {
                            let href = format!("/requests/{}", r.id.0);
                            let title = r.title.clone();
                            let status = r.status;
                            let row = class(format!(
                                "display: flex; align-items: center; gap: {g}; padding: {p} 0; border-bottom: 1px solid {b};",
                                g = space::D2, p = space::D2, b = color::BORDER,
                            ));
                            let link = class(format!(
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
    load(groups, groups_api::list());
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

fn desc_block(description: &str) -> AnyView {
    let cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; line-height: 1.55; white-space: pre-wrap;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    ));
    view! { <p class=cls>{description.to_owned()}</p> }.into_any()
}
