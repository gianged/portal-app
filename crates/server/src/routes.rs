pub mod auth;
pub mod files;

pub mod announcements;
pub mod audit;
pub mod chat;
pub mod chat_ws;
pub mod daily_reports;
pub mod day_off;
pub mod flex_hours;
pub mod groups;
pub mod holidays;
pub mod leave_balance;
pub mod notifications;
pub mod overtime;
pub mod policy;
pub mod projects;
pub mod reports;
pub mod requests;
pub mod tickets;
pub mod users;

use time::{Date, Month};

use crate::error::AppError;

/// Normalizes a `q` search parameter: trims, drops empties, and caps the
/// length (anything longer than 100 chars isn't a search, it's a payload).
pub(crate) fn norm_q(q: Option<String>) -> Option<String> {
    let trimmed = q?.trim().to_owned();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(100).collect())
}

/// Parses a `"YYYY-MM-DD"` path/query value into a [`Date`], mapping anything
/// malformed to a validation error.
pub(crate) fn parse_date(s: &str) -> Result<Date, AppError> {
    let err = || AppError::Validation(format!("invalid date '{s}', expected YYYY-MM-DD"));
    let mut parts = s.splitn(3, '-');
    let year: i32 = parts.next().ok_or_else(err)?.parse().map_err(|_| err())?;
    let month: u8 = parts.next().ok_or_else(err)?.parse().map_err(|_| err())?;
    let day: u8 = parts.next().ok_or_else(err)?.parse().map_err(|_| err())?;
    let month = Month::try_from(month).map_err(|_| err())?;
    Date::from_calendar_date(year, month, day).map_err(|_| err())
}
