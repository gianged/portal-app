//! Work-request detail: status-gated lifecycle actions, the assignee picker, and
//! attachment upload, plus the comment thread and audit trail.

use futures::FutureExt;
use futures::future::LocalBoxFuture;
use leptos::html::Input as HtmlInputEl;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use web_sys::FormData;

use shared::dto::ids::{RequestId, UserId};
use shared::dto::request::{AssignRequestRequest, RequestDetailDto, RequestDto, RequestStatus};

use crate::api::error::FrontendError;
use crate::features::audit::components::{AuditTrailPanel, TrailKind};
use crate::features::comments::{CommentTarget, CommentThread};
use crate::features::requests::api;
use crate::features::requests::components::{heading, subtle};
use crate::features::users::picker::UserPicker;
use crate::primitives::badge::Badge;
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::icon::{Icon, IconName};
use crate::primitives::stack::{Gap, Stack};
use crate::state::toast::ToastState;
use crate::theme::{class, color, space, typography};
use crate::util::format::{relative_time, request_priority_variant, request_status_variant};
use crate::util::load::{Loadable, load, load_error, note};

fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
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
            <CommentThread target=Signal::derive(move || id.get().map(CommentTarget::Request)) />
            <AuditTrailPanel
                id=Signal::derive(move || id.get().map(|r| r.0))
                kind=TrailKind::Request
                refresh=reload
            />
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
