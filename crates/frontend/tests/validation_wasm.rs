//! wasm-bindgen integration test: the frontend reuses `shared::validation` for
//! client-side checks before a network round-trip (per the Leptos frontend
//! rules), so this proves those validators link and run on the wasm target.
//!
//! `frontend` is a bin-only crate, so this integration test depends on `shared`
//! (a normal workspace dep) rather than the crate's own items.
#![cfg(target_arch = "wasm32")]

use shared::validation::ticket;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn ticket_title_rules_hold_in_wasm() {
    assert!(
        ticket::validate_ticket_title("").is_err(),
        "empty title is invalid"
    );
    assert!(
        ticket::validate_ticket_title("Printer is on fire").is_ok(),
        "a reasonable title is valid"
    );
}

#[wasm_bindgen_test]
fn ticket_description_allows_empty() {
    assert!(ticket::validate_ticket_description("").is_ok());
}
