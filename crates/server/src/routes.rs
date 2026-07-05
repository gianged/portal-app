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

use axum::extract::Multipart;
use time::{Date, Month};

use shared::validation::file;

use crate::error::AppError;

/// Reads the first multipart field of an upload as
/// `(sanitized filename, content type, bytes)`. Shared by every upload route so
/// their validation cannot drift.
pub(crate) async fn read_upload_field(
    multipart: &mut Multipart,
) -> Result<(String, String, Vec<u8>), AppError> {
    let field = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Validation(format!("invalid multipart body: {e}")))?
        .ok_or_else(|| AppError::Validation("no file field in upload".into()))?;
    let filename = field
        .file_name()
        .map(file::sanitize_filename)
        .ok_or_else(|| AppError::Validation("upload field has no filename".into()))?
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let content_type = field
        .content_type()
        .map_or_else(|| "application/octet-stream".to_owned(), ToOwned::to_owned);
    let bytes = field
        .bytes()
        .await
        .map_err(|e| AppError::Validation(format!("reading upload failed: {e}")))?
        .to_vec();
    Ok((filename, content_type, bytes))
}

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
