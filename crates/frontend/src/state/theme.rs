//! Light/dark theme state. The chosen theme is persisted to `localStorage` and
//! reflected onto `<html data-theme>`, which drives the CSS variables defined in
//! [`crate::theme::global_stylesheet`]. Provided once at the app root; the root
//! also runs an effect that calls [`apply_theme`] whenever the signal changes.

use leptos::prelude::*;

const STORAGE_KEY: &str = "portal-theme";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

impl Theme {
    #[must_use]
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }

    fn from_attr(raw: &str) -> Option<Self> {
        match raw {
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ThemeState {
    pub theme: RwSignal<Theme>,
}

impl Default for ThemeState {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeState {
    /// Seed from the persisted preference, falling back to the OS color scheme.
    #[must_use]
    pub fn new() -> Self {
        Self {
            theme: RwSignal::new(initial_theme()),
        }
    }

    pub fn toggle(&self) {
        self.theme.update(|t| *t = t.toggled());
    }

    #[must_use]
    pub fn is_dark(&self) -> bool {
        self.theme.with(|t| *t == Theme::Dark)
    }
}

/// Reflect `theme` onto `<html data-theme>` and persist it. Idempotent; safe to
/// call from an effect that tracks the theme signal (also performs the first paint).
pub fn apply_theme(theme: Theme) {
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Some(root) = window.document().and_then(|d| d.document_element()) {
        let _ = root.set_attribute("data-theme", theme.as_attr());
    }
    if let Ok(Some(storage)) = window.local_storage() {
        let _ = storage.set_item(STORAGE_KEY, theme.as_attr());
    }
}

fn initial_theme() -> Theme {
    stored_theme().unwrap_or_else(|| {
        if prefers_dark() {
            Theme::Dark
        } else {
            Theme::Light
        }
    })
}

fn stored_theme() -> Option<Theme> {
    let raw = web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(STORAGE_KEY)
        .ok()??;
    Theme::from_attr(&raw)
}

fn prefers_dark() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten())
        .is_some_and(|mql| mql.matches())
}
