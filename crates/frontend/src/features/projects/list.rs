//! Project index: an owner-group selector with the group's project cards, an incoming-invites inbox, and a create dialog.

use leptos::{prelude::*, task};
use leptos_router::components::A;
use uuid::Uuid;

use shared::dto::group::GroupDto;
use shared::dto::ids::{GroupId, ProjectInviteId};
use shared::dto::project::{
    CreateProjectRequest, ProjectDto, ProjectInviteDto, RespondInviteRequest,
};
use shared::validation::project;

use crate::features::groups::api as groups_api;
use crate::features::projects::api;
use crate::features::ui;
use crate::primitives::badge::Badge;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::chart::ProgressBar;
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
use crate::theme::{self, color, space, typography};
use crate::util::debounce;
use crate::util::format;
use crate::util::load::{self, Loadable};

#[component]
pub fn ProjectsIndex() -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let groups: Loadable<Vec<GroupDto>> = RwSignal::new(None);
    load::load(groups, groups_api::list());
    let group = RwSignal::new(None::<GroupId>);
    let projects: Loadable<Vec<ProjectDto>> = RwSignal::new(None);
    let invites: Loadable<Vec<ProjectInviteDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let create_open = RwSignal::new(false);
    let search = RwSignal::new(String::new());
    let dq = debounce::debounced(search.into(), 300);

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
        let term = dq.get().trim().to_owned();
        if let Some(g) = group.get() {
            load::load(
                projects,
                api::list_for_owner_group(g, (!term.is_empty()).then_some(term)),
            );
            load::load(invites, api::list_invites_for_group(g));
        }
    });

    let on_group = Callback::new(move |s: String| group.set(Uuid::parse_str(&s).ok().map(GroupId)));
    let group_value =
        Signal::derive(move || group.get().map(|g| g.0.to_string()).unwrap_or_default());
    let open_create = Callback::new(move |_| create_open.set(true));
    let created = Callback::new(move |()| reload.update(|n| *n += 1));
    let responded = Callback::new(move |()| reload.update(|n| *n += 1));
    let select_wrap = theme::class("width: 260px;");
    let search_wrap = theme::class("width: 220px;");

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
                <Cluster gap=Gap::Sm>
                    <div class=search_wrap>
                        <Input value=search on_input=Callback::new(move |v| search.set(v)) placeholder="Search projects…" />
                    </div>
                    <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=open_create>
                        <Icon name=IconName::Plus size=14 /> " New project"
                    </Button>
                </Cluster>
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
                    None => load::note("Loading projects…"),
                    Some(Err(e)) => load::load_error(&e),
                    Some(Ok(list)) if list.is_empty() => view! {
                        <EmptyState
                            icon=IconName::Folder
                            title="No projects yet"
                            description="This group doesn't own any projects."
                        />
                    }.into_any(),
                    Some(Ok(list)) => {
                        let grid = theme::class(format!(
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
    let progress = p.progress;
    let card_link = theme::class("text-decoration: none; display: block;");
    let name_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H3,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let desc_cls = theme::class(format!(
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
                        <Badge variant=format::project_status_variant(status)>{status.label()}</Badge>
                    </Cluster>
                    <p class=desc_cls>{desc}</p>
                    <ProgressBar value=Signal::derive(move || progress) />
                    {ui::subtle(&format!("Owned by {owner}"))}
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
        task::spawn_local(async move {
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
                    let row = theme::class(format!(
                        "display: flex; align-items: center; gap: {g}; padding: {p} 0; \
                         border-bottom: 1px solid {b};",
                        g = space::D2, p = space::D2, b = color::BORDER,
                    ));
                    let grow = theme::class("flex: 1; min-width: 0;");
                    view! {
                        <div class=row>
                            <Icon name=IconName::Folder size=14 />
                            <span class=grow>{ui::subtle(&format!("Invited by {invited_by}"))}</span>
                            <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=accept>"Accept"</Button>
                            <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=decline>"Decline"</Button>
                        </div>
                    }
                }).collect_view();
                view! { <Card><Stack gap=Gap::Sm>{ui::section_heading("Incoming invites")}{rows}</Stack></Card> }.into_any()
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
        if let Err(e) = project::validate_project_name(&n) {
            name_err.set(Some(e.to_string()));
            ok = false;
        }
        if let Err(e) = project::validate_project_description(&d) {
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
        task::spawn_local(async move {
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
                <Button variant=ButtonVariant::Primary on_click=submit disabled=Signal::derive(move || submitting.get())>
                    {move || if submitting.get() { "Creating…" } else { "Create project" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}
