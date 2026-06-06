//! Transient toast notifications. Provided once at the app root and rendered by
//! [`crate::primitives::toast::ToastHost`]. Mutation flows call
//! [`ToastState::success`] / [`ToastState::error`]; each toast auto-dismisses.

use std::time::Duration;

use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
}

#[derive(Clone)]
pub struct Toast {
    pub id: u64,
    pub message: String,
    pub kind: ToastKind,
}

/// How long a toast stays on screen before auto-dismissal.
const TOAST_TTL: Duration = Duration::from_secs(4);

#[derive(Clone, Copy)]
pub struct ToastState {
    pub items: RwSignal<Vec<Toast>>,
    next_id: RwSignal<u64>,
}

impl Default for ToastState {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            items: RwSignal::new(Vec::new()),
            next_id: RwSignal::new(0),
        }
    }

    pub fn success(&self, message: impl Into<String>) {
        self.push(ToastKind::Success, message.into());
    }

    pub fn error(&self, message: impl Into<String>) {
        self.push(ToastKind::Error, message.into());
    }

    pub fn dismiss(&self, id: u64) {
        self.items.update(|v| v.retain(|t| t.id != id));
    }

    fn push(&self, kind: ToastKind, message: String) {
        let id = self.next_id.get_untracked();
        self.next_id.set(id + 1);
        self.items.update(|v| v.push(Toast { id, message, kind }));
        let this = *self;
        set_timeout(move || this.dismiss(id), TOAST_TTL);
    }
}
