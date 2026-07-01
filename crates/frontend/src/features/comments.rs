//! Discussion comments on requests and tickets: one shared API + thread component parameterized by [`CommentTarget`].

use leptos::{prelude::*, task};

use shared::dto::comment::{CommentDto, CreateCommentRequest, UpdateCommentRequest};
use shared::dto::ids::{CommentId, RequestId, TicketId};
use shared::validation::comment;

use crate::api::client;
use crate::api::error::FrontendError;
use crate::features::ui;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::cluster::Cluster;
use crate::primitives::dialog::{Dialog, DialogBody, DialogFooter, DialogHeader};
use crate::primitives::pagination::LoadMore;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::textarea::Textarea;
use crate::state::toast::ToastState;
use crate::theme::{self, color, space, typography};
use crate::util::format;
use crate::util::load;

const PAGE: u32 = 50;

/// The work item a comment thread hangs off.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CommentTarget {
    Request(RequestId),
    Ticket(TicketId),
}

impl CommentTarget {
    fn base_path(self) -> String {
        match self {
            Self::Request(id) => format!("/requests/{}/comments", id.0),
            Self::Ticket(id) => format!("/tickets/{}/comments", id.0),
        }
    }
}

// --- api ---

async fn list(
    target: CommentTarget,
    before: Option<CommentId>,
    limit: u32,
) -> Result<Vec<CommentDto>, FrontendError> {
    let limit_s = limit.to_string();
    let path = match before {
        Some(b) => {
            let before_s = b.0.to_string();
            let q = client::query(&[("before", &before_s), ("limit", &limit_s)]);
            format!("{}{q}", target.base_path())
        }
        None => {
            let q = client::query(&[("limit", &limit_s)]);
            format!("{}{q}", target.base_path())
        }
    };
    client::get_json(&path).await
}

async fn add(
    target: CommentTarget,
    req: &CreateCommentRequest,
) -> Result<CommentDto, FrontendError> {
    client::post_json(&target.base_path(), req).await
}

async fn edit(
    target: CommentTarget,
    id: CommentId,
    req: &UpdateCommentRequest,
) -> Result<CommentDto, FrontendError> {
    client::patch_json(&format!("{}/{}", target.base_path(), id.0), req).await
}

async fn remove(target: CommentTarget, id: CommentId) -> Result<(), FrontendError> {
    client::del(&format!("{}/{}", target.base_path(), id.0)).await
}

// --- thread component ---

/// Newest-first comment timeline with a composer, backwards pagination, and grace-window edit/delete on the viewer's own comments.
#[component]
pub fn CommentThread(#[prop(into)] target: Signal<Option<CommentTarget>>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let comments = RwSignal::new(Vec::<CommentDto>::new());
    let oldest = RwSignal::new(None::<CommentId>);
    let loading_older = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let submitting = RwSignal::new(false);
    let reload = RwSignal::new(0u32);

    // Shared edit-dialog state, populated from the viewer's own rows.
    let edit_open = RwSignal::new(false);
    let edit_id = RwSignal::new(None::<CommentId>);
    let edit_body = RwSignal::new(String::new());

    Effect::new(move |_| {
        let _ = reload.get();
        let Some(t) = target.get() else { return };
        task::spawn_local(async move {
            match list(t, None, PAGE).await {
                Ok(mut page) => {
                    page.reverse();
                    oldest.set(page.first().map(|c| c.id));
                    comments.set(page);
                }
                Err(e) => toast.error_from(&e),
            }
        });
    });

    let load_older = Callback::new(move |()| {
        let Some(t) = target.get_untracked() else {
            return;
        };
        let Some(before) = oldest.get_untracked() else {
            return;
        };
        loading_older.set(true);
        task::spawn_local(async move {
            if let Ok(mut older) = list(t, Some(before), PAGE).await {
                older.reverse();
                if let Some(first) = older.first() {
                    oldest.set(Some(first.id));
                }
                if !older.is_empty() {
                    comments.update(move |v| {
                        let mut combined = older;
                        combined.append(v);
                        *v = combined;
                    });
                }
            }
            loading_older.set(false);
        });
    });

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        let Some(t) = target.get_untracked() else {
            return;
        };
        let body = draft.get_untracked();
        if let Err(e) = comment::validate_comment_body(&body) {
            toast.error(e.to_string());
            return;
        }
        submitting.set(true);
        task::spawn_local(async move {
            match add(t, &CreateCommentRequest { body }).await {
                Ok(_) => {
                    draft.set(String::new());
                    reload.update(|n| *n += 1);
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
        });
    });

    let begin_edit = move |cid: CommentId, body: String| {
        edit_id.set(Some(cid));
        edit_body.set(body);
        edit_open.set(true);
    };
    let do_delete = move |cid: CommentId| {
        let Some(t) = target.get_untracked() else {
            return;
        };
        task::spawn_local(async move {
            match remove(t, cid).await {
                Ok(()) => reload.update(|n| *n += 1),
                Err(e) => toast.error_from(&e),
            }
        });
    };

    let saved = Callback::new(move |()| reload.update(|n| *n += 1));

    view! {
        <Card>
            <Stack gap=Gap::Md>
                {ui::section_heading("Comments")}
                <Show when=move || oldest.get().is_some() && !comments.get().is_empty() fallback=|| ()>
                    <LoadMore on_click=load_older loading=loading_older.into() label="Load older" />
                </Show>
                {move || {
                    let list = comments.get();
                    if list.is_empty() {
                        load::note("No comments yet — start the discussion.")
                    } else {
                        let rows = list
                            .into_iter()
                            .map(|c| comment_row(c, begin_edit, do_delete))
                            .collect_view();
                        view! { <Stack gap=Gap::Xs>{rows}</Stack> }.into_any()
                    }
                }}
                <Stack gap=Gap::Sm>
                    <Textarea
                        value=draft
                        on_input=Callback::new(move |v| draft.set(v))
                        rows=3
                        placeholder="Write a comment…"
                    />
                    <Cluster gap=Gap::Sm justify="flex-end".to_string()>
                        <Button variant=ButtonVariant::Primary size=ButtonSize::Sm on_click=submit disabled=Signal::derive(move || submitting.get())>
                            {move || if submitting.get() { "Posting…" } else { "Comment" }}
                        </Button>
                    </Cluster>
                </Stack>
            </Stack>
        </Card>
        <CommentEditDialog open=edit_open target=target comment=edit_id body=edit_body on_saved=saved />
    }
}

fn comment_row(
    c: CommentDto,
    begin_edit: impl Fn(CommentId, String) + Copy + Send + Sync + 'static,
    do_delete: impl Fn(CommentId) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let author = c.author.full_name.clone();
    let when = format::relative_time(c.created_at);
    let edited = c.edited_at.is_some();
    let cid = c.id;
    let edit_seed = c.body.clone();
    let body = c.body.clone();

    let row = theme::class(format!(
        "display: flex; gap: {g}; padding: {py} 0; border-bottom: 1px solid {b};",
        g = space::D3,
        py = space::D2,
        b = color::BORDER,
    ));
    let bodywrap = theme::class("min-width: 0; flex: 1;");
    let meta = theme::class("display: flex; align-items: center; gap: 8px; margin-bottom: 2px;");
    let author_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let time_cls = theme::class(format!(
        "font-family: {ff}; font-size: 11.5px; color: {c};",
        ff = typography::FONT_SANS,
        c = color::TEXT_FAINT,
    ));
    let text_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; line-height: 1.5; word-wrap: break-word; \
         white-space: pre-wrap;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT,
    ));
    let when_label = if edited {
        format!("{when} · edited")
    } else {
        when
    };

    // `editable` is the server's author-within-grace verdict for this viewer.
    let controls = if c.editable {
        let actions_cls = theme::class("margin-left: auto;");
        let edit_cb = Callback::new(move |_| begin_edit(cid, edit_seed.clone()));
        let delete_cb = Callback::new(move |_| do_delete(cid));
        view! {
            <div class=actions_cls>
                <Cluster gap=Gap::Xs>
                    <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=edit_cb>"Edit"</Button>
                    <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=delete_cb>"Delete"</Button>
                </Cluster>
            </div>
        }
        .into_any()
    } else {
        ().into_any()
    };

    view! {
        <div class=row>
            <Avatar name=author.clone() size=AvatarSize::Sm tone=format::tone_for(&author) />
            <div class=bodywrap>
                <div class=meta>
                    <span class=author_cls>{author}</span>
                    <span class=time_cls>{when_label}</span>
                    {controls}
                </div>
                <div class=text_cls>{body}</div>
            </div>
        </div>
    }
}

#[component]
fn CommentEditDialog(
    open: RwSignal<bool>,
    #[prop(into)] target: Signal<Option<CommentTarget>>,
    #[prop(into)] comment: Signal<Option<CommentId>>,
    body: RwSignal<String>,
    on_saved: Callback<()>,
) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let submitting = RwSignal::new(false);
    let on_close = Callback::new(move |()| open.set(false));
    let cancel = Callback::new(move |_| open.set(false));

    let submit = Callback::new(move |_| {
        if submitting.get_untracked() {
            return;
        }
        let (Some(t), Some(cid)) = (target.get_untracked(), comment.get_untracked()) else {
            return;
        };
        let b = body.get_untracked();
        if let Err(e) = comment::validate_comment_body(&b) {
            toast.error(e.to_string());
            return;
        }
        submitting.set(true);
        let req = UpdateCommentRequest { body: b };
        task::spawn_local(async move {
            match edit(t, cid, &req).await {
                Ok(_) => {
                    open.set(false);
                    on_saved.run(());
                }
                Err(e) => toast.error_from(&e),
            }
            submitting.set(false);
        });
    });

    view! {
        <Dialog open=open on_close=on_close>
            <DialogHeader title="Edit comment" subtitle="Comments are editable for 15 minutes after posting." />
            <DialogBody>
                <Textarea value=body on_input=Callback::new(move |v| body.set(v)) rows=4 />
            </DialogBody>
            <DialogFooter>
                <Button variant=ButtonVariant::Ghost on_click=cancel>"Cancel"</Button>
                <Button variant=ButtonVariant::Primary on_click=submit disabled=Signal::derive(move || submitting.get())>
                    {move || if submitting.get() { "Saving…" } else { "Save" }}
                </Button>
            </DialogFooter>
        </Dialog>
    }
}
