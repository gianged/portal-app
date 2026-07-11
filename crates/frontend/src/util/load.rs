//! Shared async-data plumbing. A [`Loadable`] holds `None` while a one-shot
//! fetch is in flight, then `Ok`/`Err`. [`load`] kicks off the fetch;
//! [`note`] renders the loading / error / empty status lines pages share.

use leptos::{prelude::*, task};

use crate::api::display::ErrorDisplay;
use crate::api::error::FrontendError;
use crate::primitives::error::ErrorCallout;
use crate::theme::{self, color, space, typography};

/// A value fetched over the network: `None` while loading, then `Ok`/`Err`.
/// A cheap `Copy` handle; the generation counter lets overlapping loads drop
/// stale responses.
pub struct Loadable<T: Send + Sync + 'static> {
    value: RwSignal<Option<Result<T, FrontendError>>>,
    generation: StoredValue<u64>,
}

impl<T: Send + Sync + 'static> Clone for Loadable<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Send + Sync + 'static> Copy for Loadable<T> {}

impl<T: Send + Sync + 'static> Default for Loadable<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + Sync + 'static> Loadable<T> {
    /// A fresh loadable in the loading state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            value: RwSignal::new(None),
            generation: StoredValue::new(0),
        }
    }

    /// Current state, tracked.
    pub fn get(self) -> Option<Result<T, FrontendError>>
    where
        T: Clone,
    {
        self.value.get()
    }

    /// Current state without tracking.
    pub fn get_untracked(self) -> Option<Result<T, FrontendError>>
    where
        T: Clone,
    {
        self.value.get_untracked()
    }

    /// Read the current state through `f`, tracked.
    pub fn with<U>(self, f: impl FnOnce(&Option<Result<T, FrontendError>>) -> U) -> U {
        self.value.with(f)
    }

    /// Overwrite the state, notifying subscribers.
    pub fn set(self, state: Option<Result<T, FrontendError>>) {
        self.value.set(state);
    }
}

/// Reset `loadable` to loading and run `fut`, storing its result when it
/// resolves. Call again (e.g. from an `Effect` on a route param, or after a
/// mutation) to re-fetch. When loads overlap, only the newest publishes;
/// stale responses are dropped.
pub fn load<T, Fut>(loadable: Loadable<T>, fut: Fut)
where
    T: Send + Sync + 'static,
    Fut: Future<Output = Result<T, FrontendError>> + 'static,
{
    let mine = loadable.generation.get_value() + 1;
    loadable.generation.set_value(mine);
    loadable.value.set(None);
    task::spawn_local(async move {
        let result = fut.await;
        if loadable.generation.try_get_value() == Some(mine) {
            loadable.value.set(Some(result));
        }
    });
}

/// Render a load failure as a themed [`ErrorCallout`]. Use in the `Err` arm of a
/// [`Loadable`] match; `note` still serves the loading / empty lines.
#[must_use]
pub fn load_error(e: &FrontendError) -> AnyView {
    view! { <ErrorCallout display=ErrorDisplay::from(e) /> }.into_any()
}

/// A small inline status line (loading / empty). Error states use
/// [`load_error`] instead.
#[must_use]
pub fn note(text: &str) -> AnyView {
    note_view(text, space::D5)
}

/// [`note`] without the gutter, for surfaces that already pad their content.
#[must_use]
pub fn note_inline(text: &str) -> AnyView {
    note_view(text, "0")
}

fn note_view(text: &str, padding: &str) -> AnyView {
    let cls = theme::class(format!(
        "padding: {padding}; font-family: {ff}; font-size: {fs}; color: {c};",
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));
    view! { <div class=cls>{text.to_owned()}</div> }.into_any()
}
