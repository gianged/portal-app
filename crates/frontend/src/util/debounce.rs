//! Signal debouncing for type-ahead inputs (search boxes): the returned signal
//! settles to the source's value only after `ms` of quiet, so every keystroke
//! does not become an HTTP request.

use gloo::timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task;

/// Returns a signal that follows `source` with an `ms`-millisecond debounce.
/// Each change bumps a generation counter; only the timer that still matches
/// the latest generation when it fires publishes its value.
pub fn debounced(source: Signal<String>, ms: u32) -> Signal<String> {
    let out = RwSignal::new(source.get_untracked());
    let generation = StoredValue::new(0_u64);
    Effect::new(move |_| {
        let value = source.get();
        let mine = generation.with_value(|g| g + 1);
        generation.set_value(mine);
        task::spawn_local(async move {
            TimeoutFuture::new(ms).await;
            if generation.get_value() == mine {
                out.set(value);
            }
        });
    });
    out.into()
}
