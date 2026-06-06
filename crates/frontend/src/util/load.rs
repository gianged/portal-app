//! Shared async-data plumbing. A [`Loadable`] is a signal holding `None` while a
//! one-shot fetch is in flight, then `Ok`/`Err`. [`load`] kicks off the fetch;
//! [`note`] renders the loading / error / empty status lines pages share.

use std::future::Future;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::error::FrontendError;
use crate::theme::{class, color, space, typography};

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
    spawn_local(async move {
        signal.set(Some(fut.await));
    });
}

/// A small inline status line (loading / error / empty).
#[must_use]
pub fn note(text: &str, danger: bool) -> AnyView {
    let c = if danger {
        color::DANGER
    } else {
        color::TEXT_MUTED
    };
    let cls = class(format!(
        "padding: {p}; font-family: {ff}; font-size: {fs}; color: {c};",
        p = space::D5,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
    ));
    view! { <div class=cls>{text.to_owned()}</div> }.into_any()
}
