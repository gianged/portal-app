//! Daily reports UI: a "My day" editor and a leader/HR "Team daily reports"
//! review view. The server is the real authorization gate; non-leaders simply
//! see an access error when they open the team view.

use leptos::{prelude::*, task};
use uuid::Uuid;

use shared::dto::daily_report::{
    DailyReportDto, DailyReportEntryKind, DailyReportStatus, ReviewDailyReportRequest,
    UpsertDailyReportEntry, UpsertDailyReportRequest,
};
use shared::dto::group::GroupDto;
use shared::dto::ids::{DailyReportId, GroupId, RequestId};
use shared::validation::daily_report;

use crate::features::daily_reports::api;
use crate::features::groups::api as groups_api;
use crate::features::requests::api as requests_api;
use crate::primitives::badge::{Badge, BadgeVariant};
use crate::primitives::button::{Button, ButtonSize, ButtonVariant};
use crate::primitives::card::Card;
use crate::primitives::input::{FieldError, FieldLabel, Input};
use crate::primitives::select::Select;
use crate::primitives::stack::{Gap, Stack};
use crate::primitives::textarea::Textarea;
use crate::state::toast::ToastState;
use crate::theme::{self, color, space, typography};
use crate::util::date::{days_ago_iso, today_iso};
use crate::util::load::{self, Loadable};

// --- editor draft state ---

#[derive(Clone)]
struct EntryDraft {
    key: usize,
    kind: String,
    description: String,
    request_id: String,
    hours: String,
    progress: String,
}

fn kind_str(kind: DailyReportEntryKind) -> &'static str {
    match kind {
        DailyReportEntryKind::RequestWork => "request_work",
        DailyReportEntryKind::Learning => "learning",
        DailyReportEntryKind::Other => "other",
    }
}

fn kind_from_str(s: &str) -> DailyReportEntryKind {
    match s {
        "request_work" => DailyReportEntryKind::RequestWork,
        "learning" => DailyReportEntryKind::Learning,
        _ => DailyReportEntryKind::Other,
    }
}

fn parse_opt_f64(s: &str, field: &str) -> Result<Option<f64>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    s.parse::<f64>()
        .map(Some)
        .map_err(|_| format!("{field} must be a number"))
}

fn parse_opt_u8(s: &str, field: &str) -> Result<Option<u8>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    s.parse::<u8>()
        .map(Some)
        .map_err(|_| format!("{field} must be a whole number 0-100"))
}

fn build_request(
    summary: RwSignal<String>,
    entries: RwSignal<Vec<EntryDraft>>,
) -> Result<UpsertDailyReportRequest, String> {
    let mut out = Vec::new();
    for d in entries.get_untracked() {
        let kind = kind_from_str(&d.kind);
        let request_id = if kind == DailyReportEntryKind::RequestWork {
            let raw = d.request_id.trim();
            if raw.is_empty() {
                return Err("Pick a request for request-work entries".into());
            }
            Some(RequestId(
                Uuid::parse_str(raw).map_err(|_| "Invalid request selection".to_string())?,
            ))
        } else {
            None
        };
        out.push(UpsertDailyReportEntry {
            kind,
            description: d.description,
            request_id,
            hours: parse_opt_f64(&d.hours, "Hours")?,
            progress: parse_opt_u8(&d.progress, "Progress")?,
        });
    }
    Ok(UpsertDailyReportRequest {
        summary: summary.get_untracked(),
        entries: out,
    })
}

/// Replaces the editor's signals from a freshly loaded / saved report.
fn seed_from(
    dto: &DailyReportDto,
    summary: RwSignal<String>,
    entries: RwSignal<Vec<EntryDraft>>,
    status: RwSignal<Option<DailyReportStatus>>,
    report_id: RwSignal<Option<DailyReportId>>,
    next_key: RwSignal<usize>,
) {
    summary.set(dto.summary.clone());
    status.set(Some(dto.status));
    report_id.set(Some(dto.id));
    let mut start = next_key.get_untracked();
    let drafts = dto
        .entries
        .iter()
        .map(|e| {
            let key = start;
            start += 1;
            EntryDraft {
                key,
                kind: kind_str(e.kind).to_owned(),
                description: e.description.clone(),
                request_id: e.request_id.map(|r| r.0.to_string()).unwrap_or_default(),
                hours: e.hours.map(|h| h.to_string()).unwrap_or_default(),
                // progress is a transient hint, never persisted on an entry.
                progress: String::new(),
            }
        })
        .collect();
    next_key.set(start);
    entries.set(drafts);
}

fn blank(
    summary: RwSignal<String>,
    entries: RwSignal<Vec<EntryDraft>>,
    status: RwSignal<Option<DailyReportStatus>>,
    report_id: RwSignal<Option<DailyReportId>>,
) {
    summary.set(String::new());
    entries.set(Vec::new());
    status.set(None);
    report_id.set(None);
}

// --- "My day" editor ---

#[component]
pub fn MyDay() -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");

    let date = RwSignal::new(today_iso());
    let summary = RwSignal::new(String::new());
    let entries: RwSignal<Vec<EntryDraft>> = RwSignal::new(Vec::new());
    let status: RwSignal<Option<DailyReportStatus>> = RwSignal::new(None);
    let report_id: RwSignal<Option<DailyReportId>> = RwSignal::new(None);
    let next_key = RwSignal::new(0usize);
    let err = RwSignal::new(None::<String>);
    let saving = RwSignal::new(false);

    // Assigned requests for the request picker (id, title).
    let requests: RwSignal<Vec<(String, String)>> = RwSignal::new(Vec::new());
    Effect::new(move |_| {
        task::spawn_local(async move {
            match requests_api::list_mine(None, None).await {
                Ok(list) => requests.set(
                    list.into_iter()
                        .map(|r| (r.id.0.to_string(), r.title))
                        .collect(),
                ),
                Err(e) => toast.error_from(&e),
            }
        });
    });

    // Load (or blank) the report whenever the picked date changes.
    Effect::new(move |_| {
        let d = date.get();
        err.set(None);
        task::spawn_local(async move {
            match api::get_for_date(&d).await {
                Ok(Some(dto)) => {
                    seed_from(&dto, summary, entries, status, report_id, next_key);
                }
                Ok(None) => blank(summary, entries, status, report_id),
                Err(e) => err.set(Some(e.to_string())),
            }
        });
    });

    let editable = move || {
        !matches!(
            status.get(),
            Some(DailyReportStatus::Submitted | DailyReportStatus::Approved)
        )
    };

    let add_entry = move |_| {
        let key = next_key.get_untracked();
        next_key.set(key + 1);
        entries.update(|v| {
            v.push(EntryDraft {
                key,
                kind: "other".to_owned(),
                description: String::new(),
                request_id: String::new(),
                hours: String::new(),
                progress: String::new(),
            });
        });
    };

    let save = Callback::new(move |_| {
        if saving.get_untracked() {
            return;
        }
        err.set(None);
        let req = match build_request(summary, entries) {
            Ok(r) => r,
            Err(m) => {
                err.set(Some(m));
                return;
            }
        };
        if let Err(e) = daily_report::validate_daily_report(&req) {
            err.set(Some(e.to_string()));
            return;
        }
        let d = date.get_untracked();
        saving.set(true);
        task::spawn_local(async move {
            match api::upsert(&d, &req).await {
                Ok(dto) => {
                    toast.success("Draft saved");
                    seed_from(&dto, summary, entries, status, report_id, next_key);
                }
                Err(e) => {
                    toast.error_from(&e);
                    err.set(Some(e.to_string()));
                }
            }
            saving.set(false);
        });
    });

    let submit = Callback::new(move |_| {
        if saving.get_untracked() {
            return;
        }
        err.set(None);
        let req = match build_request(summary, entries) {
            Ok(r) => r,
            Err(m) => {
                err.set(Some(m));
                return;
            }
        };
        if let Err(e) = daily_report::validate_daily_report(&req) {
            err.set(Some(e.to_string()));
            return;
        }
        let d = date.get_untracked();
        saving.set(true);
        task::spawn_local(async move {
            // Persist the latest edits, then submit the resulting report.
            match api::upsert(&d, &req).await {
                Ok(dto) => match api::submit(dto.id).await {
                    Ok(s) => {
                        toast.success("Submitted for review");
                        seed_from(&s, summary, entries, status, report_id, next_key);
                    }
                    Err(e) => {
                        toast.error_from(&e);
                        err.set(Some(e.to_string()));
                        seed_from(&dto, summary, entries, status, report_id, next_key);
                    }
                },
                Err(e) => {
                    toast.error_from(&e);
                    err.set(Some(e.to_string()));
                }
            }
            saving.set(false);
        });
    });

    let head = theme::class(format!(
        "display: flex; align-items: flex-end; gap: {g}; flex-wrap: wrap;",
        g = space::D4,
    ));
    let date_box = theme::class("max-width: 200px;");

    view! {
        <Stack gap=Gap::Lg>
            <div class=head>
                <div class=date_box.clone()>
                    <FieldLabel for_id="dr-date".to_string()>"Date"</FieldLabel>
                    <Input
                        value=date
                        on_input=Callback::new(move |v| date.set(v))
                        type_="date".to_string()
                    />
                </div>
                {move || status.get().map(|s| view! { <StatusPill status=s /> })}
            </div>

            <Card>
                <Stack gap=Gap::Md>
                    <FieldLabel for_id="dr-summary".to_string()>"Summary"</FieldLabel>
                    <Textarea
                        value=summary
                        on_input=Callback::new(move |v| summary.set(v))
                        placeholder="What did you work on today?".to_string()
                        rows=3
                    />
                </Stack>
            </Card>

            <Stack gap=Gap::Sm>
                <SectionTitle title="Entries" />
                <For each=move || entries.get() key=|e| e.key let:entry>
                    {
                        let key = entry.key;
                        view! { <EntryRow key=key entries=entries requests=requests /> }
                    }
                </For>
                <div>
                    <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm on_click=Callback::new(add_entry)>
                        "+ Add entry"
                    </Button>
                </div>
            </Stack>

            {move || err.get().map(|m| view! { <FieldError message=m /> })}

            {move || if editable() {
                view! {
                    <div class=theme::class(format!("display: flex; gap: {g};", g = space::D2))>
                        <Button variant=ButtonVariant::Secondary on_click=save disabled=Signal::derive(move || saving.get())>
                            {move || if saving.get() { "Saving…" } else { "Save draft" }}
                        </Button>
                        <Button variant=ButtonVariant::Primary on_click=submit disabled=Signal::derive(move || saving.get())>
                            "Submit"
                        </Button>
                    </div>
                }.into_any()
            } else {
                load::note("This report is submitted and awaiting review.")
            }}
        </Stack>
    }
}

/// One editable entry row. Reads/writes its fields by key into the shared vec, so
/// `<For>` keeps the inputs mounted (no focus loss) while editing.
#[component]
fn EntryRow(
    key: usize,
    entries: RwSignal<Vec<EntryDraft>>,
    requests: RwSignal<Vec<(String, String)>>,
) -> impl IntoView {
    let field = move |get: fn(&EntryDraft) -> String| {
        Signal::derive(move || {
            entries.with(|v| v.iter().find(|e| e.key == key).map(get).unwrap_or_default())
        })
    };
    let kind = field(|d| d.kind.clone());
    let description = field(|d| d.description.clone());
    let request_id = field(|d| d.request_id.clone());
    let hours = field(|d| d.hours.clone());
    let progress = field(|d| d.progress.clone());

    let set_kind = Callback::new(move |v: String| {
        entries.update(|s| {
            if let Some(e) = s.iter_mut().find(|e| e.key == key) {
                e.kind = v;
            }
        });
    });
    let set_description = Callback::new(move |v: String| {
        entries.update(|s| {
            if let Some(e) = s.iter_mut().find(|e| e.key == key) {
                e.description = v;
            }
        });
    });
    let set_request = Callback::new(move |v: String| {
        entries.update(|s| {
            if let Some(e) = s.iter_mut().find(|e| e.key == key) {
                e.request_id = v;
            }
        });
    });
    let set_hours = Callback::new(move |v: String| {
        entries.update(|s| {
            if let Some(e) = s.iter_mut().find(|e| e.key == key) {
                e.hours = v;
            }
        });
    });
    let set_progress = Callback::new(move |v: String| {
        entries.update(|s| {
            if let Some(e) = s.iter_mut().find(|e| e.key == key) {
                e.progress = v;
            }
        });
    });
    let remove = move |_| entries.update(|s| s.retain(|e| e.key != key));

    let is_request_work = move || kind.get() == "request_work";

    let grid = theme::class(format!(
        "display: grid; grid-template-columns: 150px 1fr 90px auto; gap: {g}; align-items: end;",
        g = space::D3,
    ));

    view! {
        <Card>
            <Stack gap=Gap::Sm>
                <div class=grid.clone()>
                    <div>
                        <FieldLabel for_id="dr-kind".to_string()>"Kind"</FieldLabel>
                        <Select value=kind on_change=set_kind>
                            <option value="request_work">"Request work"</option>
                            <option value="learning">"Learning"</option>
                            <option value="other">"Other"</option>
                        </Select>
                    </div>
                    <div>
                        <FieldLabel for_id="dr-desc".to_string()>"Description"</FieldLabel>
                        <Input value=description on_input=set_description />
                    </div>
                    <div>
                        <FieldLabel for_id="dr-hours".to_string()>"Hours"</FieldLabel>
                        <Input value=hours on_input=set_hours type_="number".to_string() />
                    </div>
                    <Button variant=ButtonVariant::Ghost size=ButtonSize::Sm on_click=Callback::new(remove)>
                        "Remove"
                    </Button>
                </div>
                {move || is_request_work().then(|| {
                    let req_grid = theme::class(format!(
                        "display: grid; grid-template-columns: 1fr 120px; gap: {g}; align-items: end;",
                        g = space::D3,
                    ));
                    view! {
                        <div class=req_grid>
                            <div>
                                <FieldLabel for_id="dr-req".to_string()>"Request"</FieldLabel>
                                <Select value=request_id on_change=set_request>
                                    <option value="">"— select request —"</option>
                                    {move || requests.get().into_iter().map(|(id, title)| {
                                        view! { <option value=id>{title}</option> }
                                    }).collect_view()}
                                </Select>
                            </div>
                            <div>
                                <FieldLabel for_id="dr-prog".to_string()>"Progress %"</FieldLabel>
                                <Input value=progress on_input=set_progress type_="number".to_string() />
                            </div>
                        </div>
                    }
                })}
            </Stack>
        </Card>
    }
}

// --- leader / HR team review ---

#[component]
pub fn TeamReports() -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let groups: RwSignal<Vec<GroupDto>> = RwSignal::new(Vec::new());
    Effect::new(move |_| {
        task::spawn_local(async move {
            match groups_api::list().await {
                Ok(list) => groups.set(list),
                Err(e) => toast.error_from(&e),
            }
        });
    });

    let group = RwSignal::new(String::new());
    let from = RwSignal::new(days_ago_iso(7.0));
    let to = RwSignal::new(today_iso());
    let reports: Loadable<Vec<DailyReportDto>> = RwSignal::new(None);
    // Bumped after a review to force a reload.
    let tick = RwSignal::new(0u32);

    Effect::new(move |_| {
        let _ = tick.get();
        let g = group.get();
        let f = from.get();
        let t = to.get();
        let Ok(gid) = Uuid::parse_str(&g) else {
            reports.set(None);
            return;
        };
        load::load(reports, async move {
            api::list_for_group(GroupId(gid), &f, &t).await
        });
    });

    let on_reviewed = Callback::new(move |()| tick.update(|n| *n += 1));

    let head = theme::class(format!(
        "display: flex; align-items: flex-end; gap: {g}; flex-wrap: wrap;",
        g = space::D4,
    ));
    let small = theme::class("max-width: 170px;");

    view! {
        <Stack gap=Gap::Lg>
            <div class=head>
                <div class=theme::class("min-width: 220px;")>
                    <FieldLabel for_id="dr-group".to_string()>"Group"</FieldLabel>
                    <Select value=group on_change=Callback::new(move |v| group.set(v))>
                        <option value="">"— select group —"</option>
                        {move || groups.get().into_iter().map(|g| {
                            view! { <option value=g.id.0.to_string()>{g.name}</option> }
                        }).collect_view()}
                    </Select>
                </div>
                <div class=small.clone()>
                    <FieldLabel for_id="dr-from".to_string()>"From"</FieldLabel>
                    <Input value=from on_input=Callback::new(move |v| from.set(v)) type_="date".to_string() />
                </div>
                <div class=small.clone()>
                    <FieldLabel for_id="dr-to".to_string()>"To"</FieldLabel>
                    <Input value=to on_input=Callback::new(move |v| to.set(v)) type_="date".to_string() />
                </div>
            </div>

            {move || {
                if group.get().is_empty() {
                    return load::note("Pick a group to review its daily reports.");
                }
                match reports.get() {
                    None => load::note("Loading reports…"),
                    Some(Err(e)) => load::load_error(&e),
                    Some(Ok(list)) if list.is_empty() => load::note("No reports in this range."),
                    Some(Ok(list)) => view! {
                        <Stack gap=Gap::Md>
                            {list.into_iter().map(|r| view! {
                                <ReviewCard report=r on_reviewed=on_reviewed />
                            }).collect_view()}
                        </Stack>
                    }.into_any(),
                }
            }}
        </Stack>
    }
}

#[component]
fn ReviewCard(report: DailyReportDto, on_reviewed: Callback<()>) -> impl IntoView {
    let toast = use_context::<ToastState>().expect("ToastState context");
    let note = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let id = report.id;
    let reviewable = report.status == DailyReportStatus::Submitted;

    let decide = move |approve: bool| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        let req = ReviewDailyReportRequest {
            approve,
            note: note.get_untracked(),
        };
        task::spawn_local(async move {
            match api::review(id, &req).await {
                Ok(_) => {
                    toast.success(if approve {
                        "Approved"
                    } else {
                        "Returned for edits"
                    });
                    on_reviewed.run(());
                }
                Err(e) => toast.error_from(&e),
            }
            busy.set(false);
        });
    };

    let meta = theme::class(format!(
        "display: flex; align-items: center; gap: {g}; flex-wrap: wrap;",
        g = space::D3,
    ));
    let name_cls = theme::class(format!(
        "font-family: {ff}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let muted = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));

    let entries = report.entries.clone();
    let summary = report.summary.clone();
    let owner = report.user.full_name.clone();
    let date = report.report_date.clone();
    let status = report.status;

    view! {
        <Card>
            <Stack gap=Gap::Sm>
                <div class=meta>
                    <span class=name_cls>{owner}</span>
                    <span class=muted.clone()>{date}</span>
                    <StatusPill status=status />
                </div>
                {(!summary.is_empty()).then(|| view! {
                    <div class=muted.clone()>{summary}</div>
                })}
                <Stack gap=Gap::Xs>
                    {entries.into_iter().map(|e| {
                        let hours = e.hours.map(|h| format!(" · {h}h")).unwrap_or_default();
                        let line = format!("{}: {}{}", e.kind.label(), e.description, hours);
                        view! { <div class=muted.clone()>{line}</div> }
                    }).collect_view()}
                </Stack>
                {reviewable.then(|| {
                    let approve = decide;
                    view! {
                        <Stack gap=Gap::Sm>
                            <Input
                                value=note
                                on_input=Callback::new(move |v| note.set(v))
                                placeholder="Review note (optional)".to_string()
                            />
                            <div class=theme::class(format!("display: flex; gap: {g};", g = space::D2))>
                                <Button variant=ButtonVariant::Primary size=ButtonSize::Sm
                                    on_click=Callback::new(move |_| approve(true)) disabled=Signal::derive(move || busy.get())>
                                    "Approve"
                                </Button>
                                <Button variant=ButtonVariant::Secondary size=ButtonSize::Sm
                                    on_click=Callback::new(move |_| approve(false)) disabled=Signal::derive(move || busy.get())>
                                    "Return"
                                </Button>
                            </div>
                        </Stack>
                    }
                })}
            </Stack>
        </Card>
    }
}

// --- shared bits ---

#[component]
fn StatusPill(status: DailyReportStatus) -> impl IntoView {
    let variant = match status {
        DailyReportStatus::Draft => BadgeVariant::Neutral,
        DailyReportStatus::Submitted => BadgeVariant::Accent,
        DailyReportStatus::Approved => BadgeVariant::Success,
        DailyReportStatus::Returned => BadgeVariant::Danger,
    };
    view! { <Badge variant=variant>{status.label()}</Badge> }
}

fn section_title_cls() -> String {
    theme::class(format!(
        "font-size: 13px; font-weight: {fw}; color: {c}; text-transform: uppercase; \
         letter-spacing: 0.04em;",
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_MUTED,
    ))
}

#[component]
fn SectionTitle(title: &'static str) -> impl IntoView {
    view! { <div class=section_title_cls()>{title}</div> }
}
