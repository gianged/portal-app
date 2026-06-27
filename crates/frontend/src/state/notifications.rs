//! In-app notification state: the unread badge count shown on the topbar bell.
//! Populated by [`crate::features::notifications::api`] after the session bootstraps.

use leptos::prelude::*;

#[derive(Clone, Copy)]
pub struct NotificationsState {
    pub unread: RwSignal<u64>,
}

impl Default for NotificationsState {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationsState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            unread: RwSignal::new(0),
        }
    }

    pub fn set_unread(&self, count: u64) {
        self.unread.set(count);
    }
}
