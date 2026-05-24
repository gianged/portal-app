---
paths:
  - "crates/frontend/**/*.rs"
---

# Frontend (Leptos CSR) Rules

The `frontend` crate compiles to `wasm32-unknown-unknown` and runs in the browser. Its only intra-workspace dependency is `shared`. Everything else must be WASM-safe.

## WASM-only constraint

The compiler rejects most native crates, but a few sneak through and crash at runtime. Avoid:

- `std::fs`, `std::net`, `std::process` — no filesystem, no sockets, no spawning.
- `tokio` (any flavour) — there is no Tokio runtime in the browser. Async runs on the JS event loop via `wasm-bindgen-futures`.
- Native HTTP clients (`reqwest`, `hyper`, …). Use `reqwasm` (workspace dep) or `gloo-net`.

If a crate's documentation does not explicitly mention `wasm32-unknown-unknown`, assume it does not work.

## Components

Components are `#[component] fn`, PascalCase, returning `impl IntoView`:

```rust
#[component]
pub fn ProjectCard(project: ProjectDto) -> impl IntoView {
    view! {
        <Card>
            <Stack gap=Space::Sm>
                <h3>{project.name}</h3>
                <p>{project.description}</p>
            </Stack>
        </Card>
    }
}
```

- One component per file for non-trivial components; small leaf components can share a file.
- Props are owned, not borrowed — Leptos clones cheaply via signals.

## State

- Local reactive state: Leptos's reactive primitives (`RwSignal`, `Memo`, `Signal::derive`).
- Global state: declared once under `crates/frontend/src/state/`, exposed at the app root via `provide_context`, consumed with `use_context`. Examples: `AuthState`, `NotificationsState`.
- No `static mut`, no thread-locals.

## Feature folder triplet

Each domain area lives under `crates/frontend/src/features/<area>/` with three files:

- `api.rs` — typed HTTP wrappers around the backend. Each function returns `Result<T, FrontendError>` where `T` is a DTO from `shared::dto::*`.
- `components.rs` — Leptos components specific to this feature.
- `routes.rs` — route definitions consumed by `leptos_router` and wired in `app.rs`.

Larger features may split `components.rs` into more files (`features/chat/{channel_list,message_thread,composer}.rs`), but keep `api.rs` and `routes.rs` singular per feature.

## Layout and UI primitives

Compose pages from the primitives under `crates/frontend/src/primitives/`. Do not reach for raw HTML when a primitive exists:

- Layout: `Stack`, `Cluster`, `Sidebar`, `Switcher`, `Center`, `Box_`, `Grid` — not bare `<div>`.
- Inputs / surfaces: `Button`, `Input`, `Card`, `Avatar`, `Dialog`, `Dropdown`, `Tooltip` — not bare `<button>`, `<input>`, etc.

Primitives encapsulate spacing tokens, focus management, and accessibility defaults. Bypassing them creates inconsistency that is painful to untangle later — if a primitive cannot express what you need, extend the primitive, do not duplicate it inline.

## Styling

- Scoped styles via `stylist` (`style!` macro or `Style::new()`), applied as a class on the root element.
- All colour, spacing, radius, and typography values come from `crates/frontend/src/theme/`. Never hard-code hex colours or pixel values inside a component.
- Never use the `style="..."` attribute inline. If the design system can't express what you need, extend the theme tokens.

## DTOs

- Request / response types come from `shared::dto::*`. The frontend never redefines them.
- Validation types come from `shared::validation::*` and can be reused for client-side checks before the network call so the user sees errors without a round trip.

## Errors

- Each `api::*` function maps `reqwasm::Error` and any non-2xx response to `crates/frontend/src/api/error.rs::FrontendError`.
- UI surfaces handle `FrontendError` by rendering a toast or inline banner — never panic, never `.unwrap()` outside `wasm-bindgen-test` test bodies.

## WebSocket

- A single `WsClient` lives in `crates/frontend/src/api/ws.rs`.
- Constructed once in `app::build`, provided via `provide_context`. Components subscribe through the client, never by opening their own sockets.
- Reconnect logic and presence heartbeats are the client's responsibility — features stay agnostic to transport details.
