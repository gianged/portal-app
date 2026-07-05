//! Shared async-data plumbing. A [`Loadable`] is a signal holding `None` while a
//! one-shot fetch is in flight, then `Ok`/`Err`. [`load`] kicks off the fetch;
//! [`note`] renders the loading / error / empty status lines pages share.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use leptos::{prelude::*, task};

use crate::api::display::ErrorDisplay;
use crate::api::error::FrontendError;
use crate::primitives::error::ErrorCallout;
use crate::theme::{self, color, space, typography};

/// A value fetched over the network: `None` while loading, then `Ok`/`Err`.
pub type Loadable<T> = RwSignal<Option<Result<T, FrontendError>>>;

/// Latest load generation per signal; an entry exists only while a load is in flight.
static GENERATIONS: LazyLock<Mutex<HashMap<u64, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

// Signals hash their arena slot, which is unique per live signal.
fn signal_key<T: Send + Sync + 'static>(signal: Loadable<T>) -> u64 {
    let mut hasher = DefaultHasher::new();
    signal.hash(&mut hasher);
    hasher.finish()
}

/// Reset `signal` to loading and run `fut`, storing its result when it resolves.
/// Call again (e.g. from an `Effect` on a route param, or after a mutation) to
/// re-fetch. When loads overlap, only the newest publishes; stale responses are
/// dropped.
pub fn load<T, Fut>(signal: Loadable<T>, fut: Fut)
where
    T: Send + Sync + 'static,
    Fut: Future<Output = Result<T, FrontendError>> + 'static,
{
    let key = signal_key(signal);
    let mine = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
    GENERATIONS
        .lock()
        .expect("load generations lock")
        .insert(key, mine);
    signal.set(None);
    task::spawn_local(async move {
        let result = fut.await;
        let mut generations = GENERATIONS.lock().expect("load generations lock");
        if generations.get(&key) == Some(&mine) {
            generations.remove(&key);
            drop(generations);
            signal.set(Some(result));
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
    let cls = theme::class(format!(
        "padding: {p}; font-family: {ff}; font-size: {fs}; color: {c};",
        p = space::D5,
        ff = typography::FONT_SANS,
        fs = typography::TEXT_SMALL,
        c = color::TEXT_MUTED,
    ));
    view! { <div class=cls>{text.to_owned()}</div> }.into_any()
}
