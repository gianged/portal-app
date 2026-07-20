//! Transient toast notifications. Provided once at the app root and rendered by
//! [`crate::primitives::toast::ToastHost`]; each toast auto-dismisses.

use std::time::Duration;

use leptos::prelude::*;

use crate::api::display::ErrorDisplay;
use crate::api::error::FrontendError;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
}

#[derive(Clone)]
pub struct Toast {
    pub id: u64,
    /// Bold heading shown above the message; `None` for plain toasts.
    pub title: Option<String>,
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
        self.push(ToastKind::Success, None, message.into());
    }

    /// A plain error toast from a literal message (client-side guard strings).
    pub fn error(&self, message: impl Into<String>) {
        self.push(ToastKind::Error, None, message.into());
    }

    /// The primary helper for failed mutations: renders a friendly title above
    /// the human message via [`ErrorDisplay`].
    pub fn error_from(&self, err: &FrontendError) {
        let display = ErrorDisplay::from(err);
        self.push(ToastKind::Error, Some(display.title), display.message);
    }

    /// Conflict-aware variant for entity mutations: a 409 warns that the content
    /// just changed and returns `true` so the caller refetches; anything else
    /// falls back to [`Self::error_from`].
    pub fn error_or_conflict(&self, err: &FrontendError) -> bool {
        if err.is_conflict() {
            self.push(
                ToastKind::Error,
                Some("Just Updated".to_owned()),
                "Someone else just updated this content. Showing the latest version.".to_owned(),
            );
            return true;
        }
        self.error_from(err);
        false
    }

    pub fn dismiss(&self, id: u64) {
        self.items.update(|v| v.retain(|t| t.id != id));
    }

    fn push(&self, kind: ToastKind, title: Option<String>, message: String) {
        let id = self.next_id.get_untracked();
        self.next_id.set(id + 1);
        self.items.update(|v| {
            v.push(Toast {
                id,
                title,
                message,
                kind,
            });
        });
        let this = *self;
        set_timeout(move || this.dismiss(id), TOAST_TTL);
    }
}
