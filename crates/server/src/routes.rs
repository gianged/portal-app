pub mod auth;
pub mod files;

pub mod announcements;
pub mod audit;
pub mod chat;
pub mod chat_ws;
pub mod groups;
pub mod notifications;
pub mod projects;
pub mod requests;
pub mod tickets;
pub mod users;

/// Normalizes a `q` search parameter: trims, drops empties, and caps the
/// length (anything longer than 100 chars isn't a search, it's a payload).
pub(crate) fn norm_q(q: Option<String>) -> Option<String> {
    let trimmed = q?.trim().to_owned();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(100).collect())
}
