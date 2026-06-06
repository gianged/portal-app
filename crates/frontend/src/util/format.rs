//! Presentation helpers shared across features: compact relative timestamps,
//! a deterministic avatar tone, and domain-status → [`BadgeVariant`] mappings.
//!
//! Labels are not duplicated here — every status/priority enum in `shared::dto`
//! already exposes `.label()`. These mappers only choose the badge color.

use shared::dto::{
    project::ProjectStatus,
    request::{RequestPriority, RequestStatus},
    ticket::{TicketPriority, TicketStatus},
};
use time::{Month, OffsetDateTime};

use crate::primitives::badge::BadgeVariant;

/// Compact "time ago" label (`just now`, `5m`, `3h`, `2d`), falling back to a
/// short absolute date (`Jun 3`) past a week. Reads the clock via JS `Date`
/// (the `time/wasm-bindgen` feature), so it is browser-only.
#[must_use]
pub fn relative_time(ts: OffsetDateTime) -> String {
    let delta = OffsetDateTime::now_utc() - ts;
    let secs = delta.whole_seconds();
    if secs < 0 {
        return "soon".to_owned();
    }
    if secs < 45 {
        return "just now".to_owned();
    }
    let mins = delta.whole_minutes();
    if mins < 60 {
        return format!("{mins}m");
    }
    let hours = delta.whole_hours();
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = delta.whole_days();
    if days < 7 {
        return format!("{days}d");
    }
    format!("{} {}", month_abbr(ts.month()), ts.day())
}

/// Stable 0..6 avatar tone derived from a name, so the same person keeps the
/// same color. Feeds [`crate::primitives::avatar::Avatar`]'s `tone` prop.
#[must_use]
pub fn tone_for(name: &str) -> usize {
    name.bytes().map(usize::from).sum::<usize>() % 6
}

#[must_use]
pub fn request_status_variant(status: RequestStatus) -> BadgeVariant {
    match status {
        RequestStatus::Draft => BadgeVariant::Neutral,
        RequestStatus::Submitted | RequestStatus::Assigned | RequestStatus::InProgress => {
            BadgeVariant::Accent
        }
        RequestStatus::Review => BadgeVariant::Warning,
        RequestStatus::Completed => BadgeVariant::Success,
        RequestStatus::Cancelled => BadgeVariant::Danger,
    }
}

#[must_use]
pub fn ticket_status_variant(status: TicketStatus) -> BadgeVariant {
    match status {
        TicketStatus::Open | TicketStatus::Triaged | TicketStatus::Assigned => BadgeVariant::Accent,
        TicketStatus::InProgress | TicketStatus::Reopened => BadgeVariant::Warning,
        TicketStatus::Resolved => BadgeVariant::Success,
        TicketStatus::Closed => BadgeVariant::Neutral,
    }
}

#[must_use]
pub fn project_status_variant(status: ProjectStatus) -> BadgeVariant {
    match status {
        ProjectStatus::Planning => BadgeVariant::Neutral,
        ProjectStatus::Active => BadgeVariant::Success,
        ProjectStatus::OnHold => BadgeVariant::Warning,
        ProjectStatus::Completed => BadgeVariant::Accent,
        ProjectStatus::Cancelled => BadgeVariant::Danger,
    }
}

#[must_use]
pub fn request_priority_variant(priority: RequestPriority) -> BadgeVariant {
    match priority {
        RequestPriority::Low | RequestPriority::Normal => BadgeVariant::Neutral,
        RequestPriority::High => BadgeVariant::Warning,
        RequestPriority::Urgent => BadgeVariant::Danger,
    }
}

#[must_use]
pub fn ticket_priority_variant(priority: TicketPriority) -> BadgeVariant {
    match priority {
        TicketPriority::Low | TicketPriority::Normal => BadgeVariant::Neutral,
        TicketPriority::High => BadgeVariant::Warning,
        TicketPriority::Urgent => BadgeVariant::Danger,
    }
}

fn month_abbr(month: Month) -> &'static str {
    match month {
        Month::January => "Jan",
        Month::February => "Feb",
        Month::March => "Mar",
        Month::April => "Apr",
        Month::May => "May",
        Month::June => "Jun",
        Month::July => "Jul",
        Month::August => "Aug",
        Month::September => "Sep",
        Month::October => "Oct",
        Month::November => "Nov",
        Month::December => "Dec",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn tone_for_is_stable_and_bounded() {
        // Same name -> same tone, always within the 0..6 avatar palette.
        assert!(tone_for("Ada Lovelace") < 6);
        assert_eq!(tone_for("Ada Lovelace"), tone_for("Ada Lovelace"));
    }

    #[wasm_bindgen_test]
    fn relative_time_reads_browser_clock() {
        // Exercises the `time/wasm-bindgen` feature: `now_utc()` resolves through
        // the JS `Date` shim in-browser. A just-created timestamp reads "just now".
        let now = OffsetDateTime::now_utc();
        assert_eq!(relative_time(now), "just now");
    }

    #[wasm_bindgen_test]
    fn status_variants_map_terminal_states() {
        assert!(matches!(
            ticket_status_variant(TicketStatus::Resolved),
            BadgeVariant::Success
        ));
        assert!(matches!(
            request_status_variant(RequestStatus::Cancelled),
            BadgeVariant::Danger
        ));
        assert!(matches!(
            project_status_variant(ProjectStatus::Active),
            BadgeVariant::Success
        ));
    }
}
