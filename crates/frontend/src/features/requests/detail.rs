//! Work-request detail: status-gated lifecycle actions, the assignee picker, and attachment upload, plus the comment thread and audit trail.

use futures::{FutureExt, future::LocalBoxFuture};
use leptos::{ev::MouseEvent, prelude::*, task};
use web_sys::{Blob, FormData};

use shared::dto::ids::{RequestId, UserId};
use shared::dto::request::{AssignRequestRequest, RequestDetailDto, RequestDto, RequestStatus};

use crate::api::error::FrontendError;
use crate::features::audit::components::{AuditTrailPanel, TrailKind};
use crate::features::comments::{CommentTarget, CommentThread};
use crate::features::requests::api;
use crate::features::ui::{self, ProgressEditor};
use crate::features::users::picker::UserPicker;
use crate::primitives::badge::Badge;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::stack::{Gap, Stack};
use crate::state::auth::AuthState;
use crate::state::toast::ToastState;
use crate::theme::{self, color, space, typography};
use crate::util::format;
use crate::util::load::{self, Loadable};

/// What the signed-in viewer is to this request; drives which lifecycle
/// actions render. The server stays authoritative.
#[derive(Clone, Copy)]
struct ViewerCaps {
    is_creator: bool,
    is_assignee: bool,
    /// Leader or sub-leader of the owning project's owner group.
    owner_lead: bool,
}

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
    let auth = use_context::<AuthState>().expect("AuthState context");
    let detail: Loadable<RequestDetailDto> = Loadable::new();
    let reload = RwSignal::new(0u32);
    let assign_open = RwSignal::new(false);
    let assign_target = RwSignal::new(None::<UserId>);
    let assign_busy = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let file_ref: NodeRef<leptos::html::Input> = NodeRef::new();

    Effect::new(move |_| {
        let _ = reload.get();
        if let Some(rid) = id.get() {
            load::load(detail, api::get(rid));
        }
    });

    // Run a lifecycle action: resolve the future, toast, re-fetch. One at a time.
    let run = move |action: RequestAction| {
        let Some(rid) = id.get_untracked() else {
            return;
        };
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        task::spawn_local(async move {
            match action_future(action, rid).await {
                Ok(_) => {
                    toast.success("Request updated");
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
            busy.set(false);
        });
    };

    let confirm_assign = Callback::new(move |()| {
        if assign_busy.get_untracked() {
            return;
        }
        let Some(uid) = assign_target.get_untracked() else {
            toast.error("Pick someone to assign.");
            return;
        };
        let Some(rid) = id.get_untracked() else {
            return;
        };
        assign_busy.set(true);
        task::spawn_local(async move {
            let req = AssignRequestRequest {
                assignee_user_id: uid,
            };
            match api::assign(rid, &req).await {
                Ok(_) => {
                    toast.success("Request assigned");
                    assign_open.set(false);
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
            assign_busy.set(false);
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
        let blob: &Blob = file.as_ref();
        // append on a fresh FormData cannot fail
        let _ = form.append_with_blob_and_filename("file", blob, &file.name());
        task::spawn_local(async move {
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

    view! {
        <Stack gap=Gap::Lg>
            {ui::back_link("/requests", "Back to requests")}
            {move || match detail.get() {
                None => load::note("Loading request…"),
                Some(Err(e)) => load::load_error(&e),
                Some(Ok(d)) => {
                    let r = &d.request;
                    let status = r.status;
                    let priority = r.priority;
                    let progress = r.progress;
                    let title_v = ui::page_title(&r.title);
                    let meta_v = meta_line(r);
                    let desc_v = ui::desc_block(&r.description);
                    let caps = auth.user.with_untracked(|u| ViewerCaps {
                        is_creator: u.as_ref().map(|x| x.id) == Some(r.creator.id),
                        is_assignee: u.as_ref().map(|x| x.id)
                            == r.assignee.as_ref().map(|a| a.id),
                        owner_lead: auth.leads_or_subleads(d.owner_group.id),
                    });
                    let actions_v = lifecycle_bar(status, caps, run, open_assign, busy.into());
                    let attach_v = attachments_card(&d, pick_file, upload, file_ref);
                    let progress_editor = if caps.is_assignee && status == RequestStatus::InProgress {
                        if let Some(rid) = id.get_untracked() {
                            let saving = RwSignal::new(false);
                            let on_save = Callback::new(move |p: u8| {
                                saving.set(true);
                                task::spawn_local(async move {
                                    match api::set_progress(rid, p).await {
                                        Ok(_) => {
                                            toast.success("Progress updated");
                                            reload.update(|n| *n += 1);
                                        }
                                        Err(e) => toast.error_from(&e),
                                    }
                                    saving.set(false);
                                });
                            });
                            view! { <ProgressEditor initial=progress on_save=on_save saving=saving /> }.into_any()
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
                                        <Cluster gap=Gap::Xs>
                                            <Badge variant=format::request_status_variant(status)>{status.label()}</Badge>
                                            <Badge variant=format::request_priority_variant(priority)>{priority.label()}</Badge>
                                        </Cluster>
                                    </Cluster>
                                    {meta_v}
                                    {ui::progress_row(progress)}
                                    {desc_v}
                                </Stack>
                            </Card>
                            {actions_v}
                            {progress_editor}
                            {attach_v}
                        </Stack>
                    }.into_any()
                }
            }}
            <CommentThread target=Signal::derive(move || id.get().map(CommentTarget::Request)) />
            <AuditTrailPanel
                id=Signal::derive(move || id.get().map(|r| r.0))
                kind=TrailKind::Request
                refresh=reload
            />
            <AssignDialog open=assign_open target=assign_target on_confirm=confirm_assign busy=assign_busy />
        </Stack>
    }
}

fn lifecycle_bar(
    status: RequestStatus,
    caps: ViewerCaps,
    run: impl Fn(RequestAction) + Copy + Send + Sync + 'static,
    open_assign: Callback<MouseEvent>,
    busy: Signal<bool>,
) -> AnyView {
    let btn = move |label: &'static str, variant: ButtonVariant, action: RequestAction| {
        let cb = Callback::new(move |_| run(action));
        view! { <Button variant=variant size=ButtonSize::Sm on_click=cb disabled=busy>{label}</Button> }
            .into_any()
    };
    let assign = move || {
        view! {
            <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=open_assign disabled=busy>
                <Icon name=IconName::Users size=14 /> "Assign"
            </Button>
        }
        .into_any()
    };
    let cancel = move || {
        btn(
            "Cancel request",
            ButtonVariant::Destructive,
            RequestAction::Cancel,
        )
    };
    // Mirrors the server rules: submit = creator; assign = owner-group lead;
    // start/review = assignee; approve/reject = creator or lead;
    // cancel = creator, assignee, or lead.
    let can_cancel = caps.is_creator || caps.is_assignee || caps.owner_lead;

    let mut buttons: Vec<AnyView> = Vec::new();
    match status {
        RequestStatus::Draft => {
            if caps.is_creator {
                buttons.push(btn("Submit", ButtonVariant::Primary, RequestAction::Submit));
            }
            if can_cancel {
                buttons.push(cancel());
            }
        }
        RequestStatus::Submitted => {
            if caps.owner_lead {
                buttons.push(assign());
            }
            if can_cancel {
                buttons.push(cancel());
            }
        }
        RequestStatus::Assigned => {
            if caps.is_assignee {
                buttons.push(btn(
                    "Start work",
                    ButtonVariant::Primary,
                    RequestAction::Start,
                ));
            }
            if caps.owner_lead {
                buttons.push(assign());
            }
            if can_cancel {
                buttons.push(cancel());
            }
        }
        RequestStatus::InProgress => {
            if caps.is_assignee {
                buttons.push(btn(
                    "Send for review",
                    ButtonVariant::Primary,
                    RequestAction::Review,
                ));
            }
            if can_cancel {
                buttons.push(cancel());
            }
        }
        RequestStatus::Review => {
            if caps.is_creator || caps.owner_lead {
                buttons.push(btn(
                    "Approve",
                    ButtonVariant::Primary,
                    RequestAction::Approve,
                ));
                buttons.push(btn(
                    "Reject",
                    ButtonVariant::Secondary,
                    RequestAction::Reject,
                ));
            }
        }
        RequestStatus::Completed | RequestStatus::Cancelled => {}
    }

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
    #[prop(into)] busy: Signal<bool>,
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
                <Button variant=ButtonVariant::Primary on_click=confirm disabled=busy>
                    {move || if busy.get() { "Assigning…" } else { "Assign" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}

fn attachments_card(
    detail: &RequestDetailDto,
    pick_file: Callback<MouseEvent>,
    upload: Callback<()>,
    file_ref: NodeRef<leptos::html::Input>,
) -> AnyView {
    let hidden_input = theme::class("display: none;");
    let rows = detail
        .attachments
        .iter()
        .map(|a| {
            let row = theme::class(format!(
                "display: flex; align-items: center; gap: {g}; padding: {p} 0; \
                 border-bottom: 1px solid {b};",
                g = space::D2,
                p = space::D2,
                b = color::BORDER,
            ));
            let name = theme::class(format!(
                "flex: 1; min-width: 0; font-family: {ff}; font-size: {fs}; color: {c};",
                ff = typography::FONT_SANS,
                fs = typography::TEXT_SMALL,
                c = color::TEXT,
            ));
            let meta = theme::class(format!(
                "font-family: {ff}; font-size: {fs}; color: {c};",
                ff = typography::FONT_SANS,
                fs = typography::TEXT_CAPTION,
                c = color::TEXT_FAINT,
            ));
            view! {
                <div class=row>
                    <Icon name=IconName::Paperclip size=14 />
                    <span class=name>{a.filename.clone()}</span>
                    <span class=meta>{format::human_size(a.size_bytes)}</span>
                </div>
            }
        })
        .collect_view();

    let has = !detail.attachments.is_empty();
    view! {
        <Card>
            <Stack gap=Gap::Md>
                <Cluster gap=Gap::Sm justify="space-between".to_string()>
                    {ui::section_heading("Attachments")}
                    <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=pick_file>
                        <Icon name=IconName::Paperclip size=14 /> "Upload"
                    </Button>
                </Cluster>
                {if has {
                    view! { <div>{rows}</div> }.into_any()
                } else {
                    ui::subtle("No attachments yet.")
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

fn meta_line(r: &RequestDto) -> AnyView {
    let cls = theme::class(format!(
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
    let created = format::relative_time(r.created_at);
    view! {
        <div class=cls>{format!("Created by {creator} · {created} · Assignee: {assignee}")}</div>
    }
    .into_any()
}
