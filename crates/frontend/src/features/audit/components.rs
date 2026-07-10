//! Admin audit-log viewer: recent state changes across the org, plus the per-entity [`AuditTrailPanel`] embedded on detail pages.

use leptos::prelude::*;
use uuid::Uuid;

use shared::dto::audit::{AuditAction, AuditLogDto};
use shared::dto::user::UserRole;

use crate::features::audit::api;
use crate::features::ui;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::badge::{Badge, BadgeVariant};
use crate::primitives::card::Card;
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::IconName;
use crate::primitives::stack::{Gap, Stack};
use crate::state::auth::AuthState;
use crate::theme::{self, color, space, typography};
use crate::util::format;
use crate::util::load::{self, Loadable};

const PAGE: u32 = 100;
const TRAIL_PAGE: u32 = 50;

fn action_variant(action: AuditAction) -> BadgeVariant {
    match action {
        AuditAction::Create => BadgeVariant::Success,
        AuditAction::Delete => BadgeVariant::Danger,
        AuditAction::StatusChange | AuditAction::Assign | AuditAction::Transfer => {
            BadgeVariant::Accent
        }
        AuditAction::Update => BadgeVariant::Neutral,
    }
}

#[component]
pub fn AuditLogIndex() -> impl IntoView {
    let items: Loadable<Vec<AuditLogDto>> = RwSignal::new(None);
    load::load(items, api::feed(PAGE));

    view! {
        <Card>
            <Stack gap=Gap::Sm>
                {ui::section_heading("Recent activity")}
                {move || match items.get() {
                    None => load::note("Loading audit log…"),
                    Some(Err(e)) => load::load_error(&e),
                    Some(Ok(list)) if list.is_empty() => view! {
                        <EmptyState
                            icon=IconName::Clock
                            title="No activity yet"
                            description="State changes across the org will appear here."
                        />
                    }.into_any(),
                    Some(Ok(list)) => {
                        let rows = list.iter().map(audit_row).collect_view();
                        view! { <div>{rows}</div> }.into_any()
                    }
                }}
            </Stack>
        </Card>
    }
}

/// Which entity an [`AuditTrailPanel`] shows history for; dispatches to the typed api wrappers.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TrailKind {
    Request,
    Ticket,
    Project,
}

/// Per-entity audit history on detail pages (Director/HR only); `refresh` re-fetches after lifecycle actions, rows lag slightly via async projection.
#[component]
pub fn AuditTrailPanel(
    #[prop(into)] id: Signal<Option<Uuid>>,
    kind: TrailKind,
    #[prop(into)] refresh: Signal<u32>,
) -> impl IntoView {
    let auth = use_context::<AuthState>().expect("AuthState context");
    let items: Loadable<Vec<AuditLogDto>> = RwSignal::new(None);

    let is_admin = Signal::derive(move || {
        auth.user.with(|u| {
            u.as_ref()
                .is_some_and(|u| matches!(u.role, UserRole::Director | UserRole::Hr))
        })
    });

    Effect::new(move |_| {
        let _ = refresh.get();
        if !is_admin.get() {
            return;
        }
        if let Some(eid) = id.get() {
            match kind {
                TrailKind::Request => load::load(items, api::request_trail(eid, TRAIL_PAGE)),
                TrailKind::Ticket => load::load(items, api::ticket_trail(eid, TRAIL_PAGE)),
                TrailKind::Project => load::load(items, api::project_trail(eid, TRAIL_PAGE)),
            }
        }
    });

    view! {
        {move || {
            if !is_admin.get() {
                return ().into_any();
            }
            view! {
                <Card>
                    <Stack gap=Gap::Sm>
                        {ui::section_heading("History")}
                        {move || match items.get() {
                            None => load::note("Loading history…"),
                            Some(Err(e)) => load::load_error(&e),
                            Some(Ok(list)) if list.is_empty() => load::note("No recorded changes yet."),
                            Some(Ok(list)) => {
                                let rows = list.iter().map(audit_row).collect_view();
                                view! { <div>{rows}</div> }.into_any()
                            }
                        }}
                    </Stack>
                </Card>
            }
            .into_any()
        }}
    }
}

fn audit_row(log: &AuditLogDto) -> AnyView {
    // A `None` actor is a system action (or a since-deleted user).
    let actor = log
        .actor
        .as_ref()
        .map_or_else(|| "System".to_owned(), |a| a.full_name.clone());
    let when = format::relative_time(log.occurred_at);
    let entity = format!("{}.{}", log.entity_schema, log.entity_table);
    let short_id = short_uuid(&log.entity_id.to_string());
    let variant = action_variant(log.action);
    let action = log.action.label();

    let row = theme::class(format!(
        "display: flex; align-items: center; gap: {g}; padding: {p} 0; \
         border-bottom: 1px solid {b};",
        g = space::D3,
        p = space::D2,
        b = color::BORDER,
    ));
    let body = theme::class("flex: 1; min-width: 0;");
    let actor_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let meta_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    let when_cls = theme::class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c}; flex-shrink: 0;",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_TINY,
        c = color::TEXT_FAINT,
    ));

    view! {
        <div class=row>
            <Avatar name=actor.clone() size=AvatarSize::Sm tone=format::tone_for(&actor) />
            <div class=body>
                <div class=actor_cls>{actor}</div>
                <div class=meta_cls>{format!("{entity} · {short_id}")}</div>
            </div>
            <Badge variant=variant>{action}</Badge>
            <span class=when_cls>{when}</span>
        </div>
    }
    .into_any()
}

/// First dash-delimited group of a UUID, enough to disambiguate at a glance.
fn short_uuid(id: &str) -> String {
    id.split('-').next().unwrap_or(id).to_owned()
}
