//! Admin audit-log viewer: the most-recent state changes across the org.

use leptos::prelude::*;

use shared::dto::audit::{AuditAction, AuditLogDto};

use crate::features::audit::api;
use crate::features::ui::section_heading;
use crate::primitives::avatar::{Avatar, AvatarSize};
use crate::primitives::badge::{Badge, BadgeVariant};
use crate::primitives::card::Card;
use crate::primitives::empty_state::EmptyState;
use crate::primitives::icon::IconName;
use crate::primitives::stack::{Gap, Stack};
use crate::theme::{class, color, space, typography};
use crate::util::format::{relative_time, tone_for};
use crate::util::load::{Loadable, load, load_error, note};

const PAGE: u32 = 100;

fn action_variant(action: AuditAction) -> BadgeVariant {
    match action {
        AuditAction::Create => BadgeVariant::Success,
        AuditAction::Delete => BadgeVariant::Danger,
        AuditAction::StatusChange | AuditAction::Assign | AuditAction::Transfer => {
            BadgeVariant::Accent
        }
        AuditAction::Update | AuditAction::Login | AuditAction::Logout => BadgeVariant::Neutral,
    }
}

#[component]
pub fn AuditLogIndex() -> impl IntoView {
    let items: Loadable<Vec<AuditLogDto>> = RwSignal::new(None);
    load(items, api::feed(PAGE));

    view! {
        <Card>
            <Stack gap=Gap::Sm>
                {section_heading("Recent activity")}
                {move || match items.get() {
                    None => note("Loading audit log…"),
                    Some(Err(e)) => load_error(&e),
                    Some(Ok(list)) if list.is_empty() => view! {
                        <EmptyState
                            icon=IconName::Clock
                            title="No activity yet"
                            description="State changes across the org will appear here."
                        />
                    }.into_any(),
                    Some(Ok(list)) => {
                        let rows = list.into_iter().map(audit_row).collect_view();
                        view! { <div>{rows}</div> }.into_any()
                    }
                }}
            </Stack>
        </Card>
    }
}

fn audit_row(log: AuditLogDto) -> AnyView {
    // A `None` actor is a system action (or a since-deleted user).
    let actor = log
        .actor
        .as_ref()
        .map_or_else(|| "System".to_owned(), |a| a.full_name.clone());
    let when = relative_time(log.occurred_at);
    let entity = format!("{}.{}", log.entity_schema, log.entity_table);
    let short_id = short_uuid(&log.entity_id.to_string());
    let variant = action_variant(log.action);
    let action = log.action.label();

    let row = class(format!(
        "display: flex; align-items: center; gap: {g}; padding: {p} 0; \
         border-bottom: 1px solid {b};",
        g = space::D3,
        p = space::D2,
        b = color::BORDER,
    ));
    let body = class("flex: 1; min-width: 0;");
    let actor_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; font-weight: {fw}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        fw = typography::WEIGHT_SEMIBOLD,
        c = color::TEXT_STRONG,
    ));
    let meta_cls = class(format!(
        "font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_CAPTION,
        c = color::TEXT_MUTED,
    ));
    let when_cls = class(format!(
        "font-family: {ff}; font-size: 11.5px; color: {c}; flex-shrink: 0;",
        ff = typography::FONT_SANS,
        c = color::TEXT_FAINT,
    ));

    view! {
        <div class=row>
            <Avatar name=actor.clone() size=AvatarSize::Sm tone=tone_for(&actor) />
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

/// First dash-delimited group of a UUID — enough to disambiguate at a glance
/// without dumping the full id.
fn short_uuid(id: &str) -> String {
    id.split('-').next().unwrap_or(id).to_owned()
}
