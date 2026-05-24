---
paths:
  - "crates/domain/**/*.rs"
---

# Domain Layer Rules

The `domain` crate is the architectural floor — everything above depends on it, and it depends on almost nothing. These rules keep it pure so business rules stay unit-testable and the layering stays honest.

## Allowed dependencies

`std`, `serde`, `thiserror`, `time`, `uuid`, `async-trait`. That is the whole list.

`async-trait` is a proc-macro crate — it does not pull in a runtime. The trait definitions still compile without `tokio`; only the eventual impl crate needs a runtime to drive the resulting futures.

Do not add:

- `tokio` — the domain must compile without an async runtime.
- `sqlx`, `redis`, `scylla`, `reqwest`, or anything that performs IO.
- `anyhow` — domain errors are concrete types.

If a piece of logic seems to need one of these, it belongs in `application` (orchestration) or `infrastructure` (IO), not here.

## Ports are traits

A port is a trait declared under `crates/domain/src/ports/<name>.rs`. Use the `#[async_trait]` macro on every port. Native `async fn` in trait is stable (Rust 1.75+), but its desugared future is `!Send` by default and the trait is not `dyn`-compatible — both are blockers for the way services consume ports here (multi-threaded `tokio::spawn`, `Arc<dyn Trait>` for DI and test doubles).

```rust
use async_trait::async_trait;

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, id: UserId) -> Result<Option<User>, RepositoryError>;
    async fn save(&self, user: &User) -> Result<(), RepositoryError>;
}
```

Rules:

- Always declare `Send + Sync` as supertraits — implementations are shared across tasks.
- Every port trait carries `#[async_trait]`. The macro boxes each call's future (`Pin<Box<dyn Future + Send>>`) — one heap allocation per port call, the price paid for `dyn` dispatch and `Send` bounds without hand-written future types.
- Do not mix styles. Don't drop `#[async_trait]` on individual methods or rewrite some ports with `impl Future + Send` and others with the macro — pick one shape per crate and keep it.
- Return concrete error types from this crate (e.g. `RepositoryError`), never `Box<dyn Error>` and never `anyhow::Error`.
- Never name a concrete type from `infrastructure` in a port — that is the dependency inversion the layering exists to enforce.

## ID newtypes

Every entity has its own ID newtype, declared in `crates/domain/src/ids.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);
```

- Use `uuid::Uuid` v7 (time-ordered) — generate via `Uuid::now_v7()` in `application`, not here.
- Standard derive stack: `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize`.
- Never pass raw `Uuid` across module boundaries — wrap in the appropriate newtype.

## Lifecycle enums

Each entity with a state machine (User, Project, Request, Ticket) gets an enum plus transition methods that return `Result<Self, TransitionError>`:

```rust
pub enum ProjectState {
    Planning,
    Active,
    OnHold,
    Completed,
    Cancelled,
}

impl ProjectState {
    pub fn try_activate(self) -> Result<Self, TransitionError> {
        match self {
            Self::Planning => Ok(Self::Active),
            other => Err(TransitionError::invalid(other, "activate")),
        }
    }
}
```

- Variants only — never strings. Serialise via serde's default tag.
- One `try_<verb>` method per legal transition. Invalid transitions return `TransitionError`; they do not panic.
- The enum and its transition methods own the state machine. Application services call them; they do not reimplement the rules.

## Time

`time::OffsetDateTime` for every timestamp.

- Never `time::PrimitiveDateTime` — no timezone is a bug, not a feature.
- Never `chrono` — pick one time crate per workspace and stay there.
- Read the clock (`OffsetDateTime::now_utc()`) only in `application`. In `domain`, accept the timestamp as a parameter so logic stays deterministic in tests.

## Errors

Each module / port exposes its own error enum via `thiserror`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("entity not found")]
    NotFound,
    #[error("conflicting state: {0}")]
    Conflict(String),
    #[error("backend error: {0}")]
    Backend(String),
}
```

- One error enum per module — do not define a giant cross-module union.
- `Backend(String)` is the catch-all for whatever the infrastructure adapter wrapped; keep the message short.
- Never `anyhow::Error`. Never `Box<dyn Error>`. Never a plain `String` as the error type.
