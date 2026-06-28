//! Shared async-data plumbing. A [`Loadable`] is a signal holding `None` while a
//! one-shot fetch is in flight, then `Ok`/`Err`. [`load`] kicks off the fetch;
//! [`note`] renders the loading / error / empty status lines pages share.

use leptos::{prelude::*, task};

use crate::api::display::ErrorDisplay;
use crate::api::error::FrontendError;
use crate::primitives::error::ErrorCallout;
use crate::theme::{self, color, space, typography};

/// A value fetched over the network: `None` while loading, then `Ok`/`Err`.
pub type Loadable<T> = RwSignal<Option<Result<T, FrontendError>>>;

/// Reset `signal` to loading and run `fut`, storing its result when it resolves.
/// Call again (e.g. from an `Effect` on a route param, or after a mutation) to
/// re-fetch.
pub fn load<T, Fut>(signal: Loadable<T>, fut: Fut)
where
    T: Send + Sync + 'static,
    Fut: Future<Output = Result<T, FrontendError>> + 'static,
{
    signal.set(None);
    task::spawn_local(async move {
        signal.set(Some(fut.await));
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
    let cls = theme::class(format!(
        "padding: {p}; font-family: {ff}; font-size: {fs}; color: {c};",
        p = space::D5,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));
    view! { <div class=cls>{text.to_owned()}</div> }.into_any()
}
