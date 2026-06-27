//! Work-request index: the "assigned to me" table with a create dialog and a group to project cascade picker.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use uuid::Uuid;

use shared::dto::group::GroupDto;
use shared::dto::ids::{GroupId, ProjectId};
use shared::dto::project::ProjectDto;
use shared::dto::request::{CreateRequestRequest, RequestDto, RequestPriority, RequestStatus};
use shared::validation::request::{validate_request_description, validate_request_title};

use crate::features::groups::api as groups_api;
use crate::features::projects::api as projects_api;
use crate::features::requests::api;
use crate::features::requests::components::{heading, subtle};
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::badge::Badge;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::input::{FieldError, FieldLabel, Input};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::table::{Table, TableToolbar, TableWrap};
use crate::primitives::textarea::Textarea;
use crate::state::toast::ToastState;
use crate::theme::{class, color, space, typography};
use crate::util::debounce::debounced;
use crate::util::format::{
    relative_time, request_priority_variant, request_status_variant, tone_for,
};
use crate::util::load::{Loadable, load, load_error, note};

const ALL_STATUSES: [RequestStatus; 7] = [
    RequestStatus::Draft,
    RequestStatus::Submitted,
    RequestStatus::Assigned,
    RequestStatus::InProgress,
    RequestStatus::Review,
    RequestStatus::Completed,
    RequestStatus::Cancelled,
];

fn status_wire(s: RequestStatus) -> &'static str {
    match s {
        RequestStatus::Draft => "draft",
        RequestStatus::Submitted => "submitted",
        RequestStatus::Assigned => "assigned",
        RequestStatus::InProgress => "in_progress",
        RequestStatus::Review => "review",
        RequestStatus::Completed => "completed",
        RequestStatus::Cancelled => "cancelled",
    }
}

fn status_from_wire(s: &str) -> Option<RequestStatus> {
    ALL_STATUSES.into_iter().find(|st| status_wire(*st) == s)
}

fn priority_wire(p: RequestPriority) -> &'static str {
    match p {
        RequestPriority::Low => "low",
        RequestPriority::Normal => "normal",
        RequestPriority::High => "high",
        RequestPriority::Urgent => "urgent",
    }
}

fn priority_from_wire(s: &str) -> RequestPriority {
    match s {
        "low" => RequestPriority::Low,
        "high" => RequestPriority::High,
        "urgent" => RequestPriority::Urgent,
        _ => RequestPriority::Normal,
    }
}

fn short_id(id: &Uuid) -> String {
    let s = id.to_string();
    format!("#{}", s.get(..8).unwrap_or(&s))
}

#[component]
pub fn RequestsIndex() -> impl IntoView {
    let status = RwSignal::new(None::<RequestStatus>);
    let items: Loadable<Vec<RequestDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let create_open = RwSignal::new(false);
    let search = RwSignal::new(String::new());
    let dq = debounced(search.into(), 300);

    Effect::new(move |_| {
        let _ = reload.get();
        let term = dq.get().trim().to_owned();
        load(
            items,
            api::list_mine(status.get(), (!term.is_empty()).then_some(term)),
        );
    });

    let on_status = Callback::new(move |v: String| status.set(status_from_wire(&v)));
    let status_value =
        Signal::derive(move || status.get().map(status_wire).unwrap_or_default().to_owned());
    let open_create = Callback::new(move |_| create_open.set(true));
    let created = Callback::new(move |()| reload.update(|n| *n += 1));
    let select_wrap = class("width: 170px;");
    let search_wrap = class("width: 220px;");

    view! {
        <Stack gap=Gap::Lg>
            <TableWrap>
                <TableToolbar>
                    <Stack gap=Gap::Xs>
                        {heading("Requests assigned to you")}
                        {subtle("Work requests where you're the assignee")}
                    </Stack>
                    <Cluster gap=Gap::Sm>
                        <div class=search_wrap>
                            <Input value=search on_input=Callback::new(move |v| search.set(v)) placeholder="Search requests…" />
                        </div>
                        <div class=select_wrap>
                            <Select value=status_value on_change=on_status>
                                <option value="">"All statuses"</option>
                                {ALL_STATUSES.into_iter().map(|s| view! {
                                    <option value=status_wire(s)>{s.label()}</option>
                                }).collect_view()}
                            </Select>
                        </div>
                        <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=open_create>
                            <Icon name=IconName::Plus size=14 /> " New request"
                        </Button>
                    </Cluster>
                </TableToolbar>
                {move || match items.get() {
                    None => note("Loading requests…"),
                    Some(Err(e)) => load_error(&e),
                    Some(Ok(list)) if list.is_empty() => view! {
                        <EmptyState
                            icon=IconName::Doc
                            title="Nothing assigned to you"
                            description="Requests assigned to you show up here."
                        />
                    }.into_any(),
                    Some(Ok(list)) => requests_table(list),
                }}
            </TableWrap>
            <CreateRequestDialog open=create_open on_created=created />
        </Stack>
    }
}

fn requests_table(items: Vec<RequestDto>) -> AnyView {
    view! {
        <Table>
            <thead>
                <tr>
                    <th>"ID"</th>
                    <th>"Title"</th>
                    <th>"Status"</th>
                    <th>"Priority"</th>
                    <th>"Assignee"</th>
                    <th>"Updated"</th>
                </tr>
            </thead>
            <tbody>
                {items.into_iter().map(request_row).collect_view()}
            </tbody>
        </Table>
    }
    .into_any()
}

fn request_row(r: RequestDto) -> impl IntoView {
    let href = format!("/requests/{}", r.id.0);
    let id_label = short_id(&r.id.0);
    let title = r.title.clone();
    let status = r.status;
    let priority = r.priority;
    let updated = relative_time(r.updated_at);
    let assignee = r.assignee.map(|a| a.full_name);
    let link_cls = class(format!(
        "color: {c}; font-weight: {fw}; text-decoration: none; &:hover {{ color: {a}; }}",
        c = color::TEXT_STRONG,
        fw = typography::WEIGHT_MEDIUM,
        a = color::ACCENT,
    ));
    view! {
        <tr>
            <td><span class="mono cell-muted">{id_label}</span></td>
            <td><A href=href attr:class=link_cls>{title}</A></td>
            <td><Badge variant=request_status_variant(status)>{status.label()}</Badge></td>
            <td><Badge variant=request_priority_variant(priority)>{priority.label()}</Badge></td>
            <td>{match assignee {
                Some(name) => assignee_cell(&name),
                None => view! { <span class="cell-muted">"—"</span> }.into_any(),
            }}</td>
            <td><span class="cell-muted">{updated}</span></td>
        </tr>
    }
}

fn assignee_cell(name: &str) -> AnyView {
    let wrap = class(format!(
        "display: inline-flex; align-items: center; gap: {g};",
        g = space::D2
    ));
    view! {
        <span class=wrap>
            <Avatar name=name.to_owned() size=AvatarSize::Sm tone=tone_for(name) />
            <span class="cell-strong">{name.to_owned()}</span>
        </span>
    }
    .into_any()
}

#[component]
fn CreateRequestDialog(open: RwSignal<bool>, on_created: Callback<()>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let title = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let priority = RwSignal::new(RequestPriority::Normal);
    let project = RwSignal::new(None::<ProjectId>);
    let title_err = RwSignal::new(None::<String>);
    let desc_err = RwSignal::new(None::<String>);
    let submitting = RwSignal::new(false);

    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_priority = Callback::new(move |v: String| priority.set(priority_from_wire(&v)));
    let priority_value = Signal::derive(move || priority_wire(priority.get()).to_owned());

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        title_err.set(None);
        desc_err.set(None);
        let t = title.get_untracked();
        let d = description.get_untracked();
        let mut ok = true;
        if let Err(e) = validate_request_title(&t) {
            title_err.set(Some(e.to_string()));
            ok = false;
        }
        if let Err(e) = validate_request_description(&d) {
            desc_err.set(Some(e.to_string()));
            ok = false;
        }
        let Some(pid) = project.get_untracked() else {
            toast.error("Pick a project for this request.");
            return;
        };
        if !ok {
            return;
        }
        submitting.set(true);
        let req = CreateRequestRequest {
            project_id: pid,
            title: t,
            description: d,
            priority: priority.get_untracked(),
            due_at: None,
        };
        spawn_local(async move {
            match api::create(&req).await {
                Ok(_) => {
                    toast.success("Request created");
                    title.set(String::new());
                    description.set(String::new());
                    priority.set(RequestPriority::Normal);
                    project.set(None);
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
            <DialogHeader title="New request" subtitle="Create a work request against a project." />
            <DialogBody>
                <Stack gap=Gap::Lg>
                    <div>
                        <FieldLabel for_id="req-title">"Title"</FieldLabel>
                        <Input
                            value=title
                            on_input=Callback::new(move |v| title.set(v))
                            placeholder="Short summary"
                        />
                        {move || title_err.get().map(|m| view! { <FieldError message=m /> })}
                    </div>
                    <div>
                        <FieldLabel for_id="req-desc">"Description"</FieldLabel>
                        <Textarea
                            value=description
                            on_input=Callback::new(move |v| description.set(v))
                            placeholder="What needs to happen and why"
                        />
                        {move || desc_err.get().map(|m| view! { <FieldError message=m /> })}
                    </div>
                    <div>
                        <FieldLabel for_id="req-project">"Project"</FieldLabel>
                        <ProjectPicker selected=project />
                    </div>
                    <div>
                        <FieldLabel for_id="req-priority">"Priority"</FieldLabel>
                        <Select value=priority_value on_change=on_priority>
                            <option value="low">"Low"</option>
                            <option value="normal">"Normal"</option>
                            <option value="high">"High"</option>
                            <option value="urgent">"Urgent"</option>
                        </Select>
                    </div>
                </Stack>
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=submit disabled=submitting.get()>
                    {move || if submitting.get() { "Creating…" } else { "Create request" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}

/// Group to project cascade; writes the chosen project into `selected`.
#[component]
fn ProjectPicker(selected: RwSignal<Option<ProjectId>>) -> impl IntoView {
    let groups: Loadable<Vec<GroupDto>> = RwSignal::new(None);
    load(groups, groups_api::list());
    let group = RwSignal::new(None::<GroupId>);
    let projects: Loadable<Vec<ProjectDto>> = RwSignal::new(None);

    Effect::new(move |_| {
        if let Some(g) = group.get() {
            selected.set(None);
            load(projects, projects_api::list_for_owner_group(g, None));
        }
    });

    let on_group = Callback::new(move |s: String| group.set(Uuid::parse_str(&s).ok().map(GroupId)));
    let on_project =
        Callback::new(move |s: String| selected.set(Uuid::parse_str(&s).ok().map(ProjectId)));
    let group_value =
        Signal::derive(move || group.get().map(|g| g.0.to_string()).unwrap_or_default());
    let project_value =
        Signal::derive(move || selected.get().map(|p| p.0.to_string()).unwrap_or_default());

    view! {
        <Stack gap=Gap::Sm>
            <Select value=group_value on_change=on_group>
                <option value="">"Select a group…"</option>
                {move || groups.get().and_then(Result::ok).map(|l| {
                    l.into_iter().map(|g| {
                        let id = g.id.0.to_string();
                        view! { <option value=id>{g.name}</option> }
                    }).collect_view()
                })}
            </Select>
            <Select value=project_value on_change=on_project>
                <option value="">"Select a project…"</option>
                {move || projects.get().and_then(Result::ok).map(|l| {
                    l.into_iter().map(|p| {
                        let id = p.id.0.to_string();
                        view! { <option value=id>{p.name}</option> }
                    }).collect_view()
                })}
            </Select>
        </Stack>
    }
}
