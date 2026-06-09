//! Work-request UI: the "assigned to me" index with a create dialog, and the
//! detail view with its status-gated lifecycle actions, assignee picker, and
//! attachment upload.

use futures::FutureExt;
use futures::future::LocalBoxFuture;
use leptos::html::Input as HtmlInputEl;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use uuid::Uuid;
use web_sys::FormData;

use shared::dto::group::GroupDto;
use shared::dto::ids::{GroupId, ProjectId, RequestId, UserId};
use shared::dto::project::ProjectDto;
use shared::dto::request::{
    AssignRequestRequest, CreateRequestRequest, RequestDetailDto, RequestDto, RequestPriority,
    RequestStatus,
};
use shared::validation::request::{validate_request_description, validate_request_title};

use crate::api::error::FrontendError;
use crate::features::groups::api as groups_api;
use crate::features::projects::api as projects_api;
use crate::features::requests::api;
use crate::features::users::components::UserPicker;
use crate::primitives::avatar::{Avatar, AvatarSize};
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
use crate::primitives::table::{Table, TableToolbar, TableWrap};
use crate::primitives::textarea::Textarea;
use crate::state::toast::ToastState;
use crate::theme::{class, color, space, typography};
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

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn short_id(id: &Uuid) -> String {
    let s = id.to_string();
    format!("#{}", s.get(..8).unwrap_or(&s))
}

// ─────────────────────────── Index ───────────────────────────

#[component]
pub fn RequestsIndex() -> impl IntoView {
    let status = RwSignal::new(None::<RequestStatus>);
    let items: Loadable<Vec<RequestDto>> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let create_open = RwSignal::new(false);

    Effect::new(move |_| {
        let _ = reload.get();
        load(items, api::list_mine(status.get()));
    });

    let on_status = Callback::new(move |v: String| status.set(status_from_wire(&v)));
    let status_value =
        Signal::derive(move || status.get().map(status_wire).unwrap_or_default().to_owned());
    let open_create = Callback::new(move |_| create_open.set(true));
    let created = Callback::new(move |()| reload.update(|n| *n += 1));
    let select_wrap = class("width: 170px;");

    view! {
        <Stack gap=Gap::Lg>
            <TableWrap>
                <TableToolbar>
                    <Stack gap=Gap::Xs>
                        {heading("Requests assigned to you")}
                        {subtle("Work requests where you're the assignee")}
                    </Stack>
                    <Cluster gap=Gap::Sm>
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

// ─────────────────────────── Create dialog ───────────────────────────

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

/// Group → project cascade. Writes the chosen project into `selected`.
#[component]
fn ProjectPicker(selected: RwSignal<Option<ProjectId>>) -> impl IntoView {
    let groups: Loadable<Vec<GroupDto>> = RwSignal::new(None);
    load(groups, groups_api::list());
    let group = RwSignal::new(None::<GroupId>);
    let projects: Loadable<Vec<ProjectDto>> = RwSignal::new(None);

    Effect::new(move |_| {
        if let Some(g) = group.get() {
            selected.set(None);
            load(projects, projects_api::list_for_owner_group(g));
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

// ─────────────────────────── Detail ───────────────────────────

#[derive(Clone, Copy)]
enum RequestAction {
    Submit,
    Start,
    Review,
    Approve,
    Reject,
    Cancel,
}

fn action_future(
    action: RequestAction,
    id: RequestId,
) -> LocalBoxFuture<'static, Result<RequestDto, FrontendError>> {
    match action {
        RequestAction::Submit => api::submit(id).boxed_local(),
        RequestAction::Start => api::start(id).boxed_local(),
        RequestAction::Review => api::send_for_review(id).boxed_local(),
        RequestAction::Approve => api::approve(id).boxed_local(),
        RequestAction::Reject => api::reject(id).boxed_local(),
        RequestAction::Cancel => api::cancel(id).boxed_local(),
    }
}

#[component]
pub fn RequestDetail(#[prop(into)] id: Signal<Option<RequestId>>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let detail: Loadable<RequestDetailDto> = RwSignal::new(None);
    let reload = RwSignal::new(0u32);
    let assign_open = RwSignal::new(false);
    let assign_target = RwSignal::new(None::<UserId>);
    let file_ref: NodeRef<HtmlInputEl> = NodeRef::new();

    Effect::new(move |_| {
        let _ = reload.get();
        if let Some(rid) = id.get() {
            load(detail, api::get(rid));
        }
    });

    // Run a lifecycle action: resolve the future, toast, re-fetch.
    let run = move |action: RequestAction| {
        let Some(rid) = id.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match action_future(action, rid).await {
                Ok(_) => {
                    toast.success("Request updated");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
        });
    };

    let confirm_assign = Callback::new(move |()| {
        let Some(uid) = assign_target.get_untracked() else {
            toast.error("Pick someone to assign.");
            return;
        };
        let Some(rid) = id.get_untracked() else {
            return;
        };
        assign_open.set(false);
        spawn_local(async move {
            let req = AssignRequestRequest {
                assignee_user_id: uid,
            };
            match api::assign(rid, &req).await {
                Ok(_) => {
                    toast.success("Request assigned");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
        });
    });

    let upload = Callback::new(move |()| {
        let Some(input) = file_ref.get() else { return };
        let Some(files) = input.files() else { return };
        let Some(file) = files.get(0) else {
            toast.error("Choose a file first.");
            return;
        };
        let Some(rid) = id.get_untracked() else {
            return;
        };
        let form = FormData::new().expect("FormData is constructible in the browser");
        let blob: &web_sys::Blob = file.as_ref();
        let _ = form.append_with_blob_and_filename("file", blob, &file.name());
        spawn_local(async move {
            match api::upload_attachment(rid, form).await {
                Ok(_) => {
                    toast.success("Attachment uploaded");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
        });
    });

    let pick_file = Callback::new(move |_| {
        if let Some(input) = file_ref.get() {
            input.click();
        }
    });
    let open_assign = Callback::new(move |_| assign_open.set(true));

    let back_cls = class(format!(
        "display: inline-flex; align-items: center; gap: 4px; font-family: {ff}; \
         font-size: {fs}; color: {c}; text-decoration: none; &:hover {{ color: {a}; }}",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
        a = color::ACCENT,
    ));

    view! {
        <Stack gap=Gap::Lg>
            <A href="/requests" attr:class=back_cls>
                <Icon name=IconName::ChevronLeft size=14 /> "Back to requests"
            </A>
            {move || match detail.get() {
                None => note("Loading request…"),
                Some(Err(e)) => load_error(&e),
                Some(Ok(d)) => {
                    let r = &d.request;
                    let status = r.status;
                    let priority = r.priority;
                    let title_v = title_block(&r.title);
                    let meta_v = meta_line(r);
                    let desc_v = desc_block(&r.description);
                    let actions_v = lifecycle_bar(status, run, open_assign);
                    let attach_v = attachments_card(&d, pick_file, upload, file_ref);
                    view! {
                        <Stack gap=Gap::Lg>
                            <Card>
                                <Stack gap=Gap::Md>
                                    <Cluster gap=Gap::Sm justify="space-between".to_string()>
                                        {title_v}
                                        <Cluster gap=Gap::Xs>
                                            <Badge variant=request_status_variant(status)>{status.label()}</Badge>
                                            <Badge variant=request_priority_variant(priority)>{priority.label()}</Badge>
                                        </Cluster>
                                    </Cluster>
                                    {meta_v}
                                    {desc_v}
                                </Stack>
                            </Card>
                            {actions_v}
                            {attach_v}
                        </Stack>
                    }.into_any()
                }
            }}
            <AssignDialog open=assign_open target=assign_target on_confirm=confirm_assign />
        </Stack>
    }
}

fn lifecycle_bar(
    status: RequestStatus,
    run: impl Fn(RequestAction) + Copy + Send + Sync + 'static,
    open_assign: Callback<leptos::ev::MouseEvent>,
) -> AnyView {
    let btn = move |label: &'static str, variant: ButtonVariant, action: RequestAction| {
        let cb = Callback::new(move |_| run(action));
        view! { <Button variant=variant size=ButtonSize::Sm on_click=cb>{label}</Button> }
            .into_any()
    };
    let assign = move || {
        view! {
            <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=open_assign>
                <Icon name=IconName::Users size=14 /> " Assign"
            </Button>
        }
        .into_any()
    };

    let buttons: Vec<AnyView> = match status {
        RequestStatus::Draft => vec![
            btn("Submit", ButtonVariant::Primary, RequestAction::Submit),
            btn(
                "Cancel request",
                ButtonVariant::Destructive,
                RequestAction::Cancel,
            ),
        ],
        RequestStatus::Submitted => vec![
            assign(),
            btn(
                "Cancel request",
                ButtonVariant::Destructive,
                RequestAction::Cancel,
            ),
        ],
        RequestStatus::Assigned => vec![
            btn("Start work", ButtonVariant::Primary, RequestAction::Start),
            assign(),
            btn(
                "Cancel request",
                ButtonVariant::Destructive,
                RequestAction::Cancel,
            ),
        ],
        RequestStatus::InProgress => vec![
            btn(
                "Send for review",
                ButtonVariant::Primary,
                RequestAction::Review,
            ),
            btn(
                "Cancel request",
                ButtonVariant::Destructive,
                RequestAction::Cancel,
            ),
        ],
        RequestStatus::Review => vec![
            btn("Approve", ButtonVariant::Primary, RequestAction::Approve),
            btn("Reject", ButtonVariant::Secondary, RequestAction::Reject),
        ],
        RequestStatus::Completed | RequestStatus::Cancelled => Vec::new(),
    };

    if buttons.is_empty() {
        return ().into_any();
    }
    view! { <Card><Cluster gap=Gap::Sm>{buttons}</Cluster></Card> }.into_any()
}

#[component]
fn AssignDialog(
    open: RwSignal<bool>,
    target: RwSignal<Option<UserId>>,
    on_confirm: Callback<()>,
) -> impl IntoView {
    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));
    let on_select = Callback::new(move |u: UserId| target.set(Some(u)));
    let confirm = Callback::new(move |_| on_confirm.run(()));
    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Assign request" subtitle="Choose who should own this request." />
            <DialogBody>
                <UserPicker selected=target on_select=on_select />
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=confirm>"Assign"</Button>
            </DialogFooter>
        </Dialog>
    }
}

fn attachments_card(
    detail: &RequestDetailDto,
    pick_file: Callback<leptos::ev::MouseEvent>,
    upload: Callback<()>,
    file_ref: NodeRef<HtmlInputEl>,
) -> AnyView {
    let hidden_input = class("display: none;");
    let rows = detail
        .attachments
        .iter()
        .map(|a| {
            let row = class(format!(
                "display: flex; align-items: center; gap: {g}; padding: {p} 0; \
                 border-bottom: 1px solid {b};",
                g = space::D2,
                p = space::D2,
                b = color::BORDER,
            ));
            let name = class(format!(
                "flex: 1; min-width: 0; font-family: {ff}; font-size: {fs}; color: {c};",
                ff = typography::FONT_SANS,
                fs = typography::TEXT_SMALL,
                c = color::TEXT,
            ));
            let meta = class(format!(
                "font-family: {ff}; font-size: {fs}; color: {c};",
                ff = typography::FONT_SANS,
                fs = typography::TEXT_CAPTION,
                c = color::TEXT_FAINT,
            ));
            view! {
                <div class=row>
                    <Icon name=IconName::Paperclip size=14 />
                    <span class=name>{a.filename.clone()}</span>
                    <span class=meta>{human_size(a.size_bytes)}</span>
                </div>
            }
        })
        .collect_view();

    let has = !detail.attachments.is_empty();
    view! {
        <Card>
            <Stack gap=Gap::Md>
                <Cluster gap=Gap::Sm justify="space-between".to_string()>
                    {heading("Attachments")}
                    <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=pick_file>
                        <Icon name=IconName::Paperclip size=14 /> " Upload"
                    </Button>
                </Cluster>
                {if has {
                    view! { <div>{rows}</div> }.into_any()
                } else {
                    subtle("No attachments yet.")
                }}
                <input
                    type="file"
                    node_ref=file_ref
                    class=hidden_input
                    on:change=move |_| upload.run(())
                />
            </Stack>
        </Card>
    }
    .into_any()
}

// ─────────────────────────── shared bits ───────────────────────────

fn title_block(title: &str) -> AnyView {
    let cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0; \
         letter-spacing: -0.015em;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_H2,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    view! { <h2 class=cls>{title.to_owned()}</h2> }.into_any()
}

fn meta_line(r: &RequestDto) -> AnyView {
    let cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    let creator = r.creator.full_name.clone();
    let assignee = r
        .assignee
        .as_ref()
        .map_or_else(|| "Unassigned".to_owned(), |a| a.full_name.clone());
    let created = relative_time(r.created_at);
    view! {
        <div class=cls>{format!("Created by {creator} · {created} · Assignee: {assignee}")}</div>
    }
    .into_any()
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

fn heading(text: &str) -> AnyView {
    let cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c}; margin: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_BODY,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    view! { <h3 class=cls>{text.to_owned()}</h3> }.into_any()
}

fn subtle(text: &str) -> AnyView {
    let cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    view! { <div class=cls>{text.to_owned()}</div> }.into_any()
}
