# Portal

Internal company portal for one organization (100–1000 users). Full-stack Rust: Axum HTTP/WebSocket backend, Leptos WebAssembly frontend, ReBAC authorization via OpenFGA. Surfaces: project tracking, work requests, IT tickets, real-time chat, company-wide announcements.

## Status

The backend is substantially implemented inward-out: `domain`, `application`, `infrastructure`, and `server` are largely complete, and `shared` DTOs/validation are mostly in place. `workers` (job handlers) and `frontend` (most feature modules) are still partial — scaffolded with a few flows live. Treat empty modules as not-yet-written, not broken.

## Toolchain

- Edition 2024, MSRV **1.94** (declared as `rust-version` in the workspace `Cargo.toml`). Develop with the latest stable toolchain; CI gates on MSRV.
- Targets: native + `wasm32-unknown-unknown`.
- TLS via `rustls`. Never add `openssl` or `native-tls`.
- Async runtime: `tokio`. Background jobs: `apalis`.

## Workspace layout

```
crates/
├── domain          Pure types + repository/ports (traits). No async, no IO, no frameworks.
├── application     Business services. Depends only on `domain`. Async via tokio.
├── infrastructure  Adapters: sqlx/Postgres, scylla/Cassandra, redis, openfga, local fs.
├── shared          DTOs + validation. Compiles for native AND wasm32.
├── server          Axum binary (HTTP + WebSocket). HTTP composition root.
├── workers         Apalis binary. Background-job composition root.
└── frontend        Leptos CSR SPA, built with Trunk to WebAssembly.
```

## Architectural layering

The dependency graph is one-directional and Cargo enforces it:

```
              ┌── application ───┐
domain ◄──────┤                  ├── server
              └── infrastructure ┘   workers

shared ◄── frontend, server, workers
```

Rules:

- `domain` — std + `serde` + `thiserror` + `time` + `uuid` only. No async traits, no `tokio`, no IO. Ports are plain traits.
- `application` — consumes `domain` and orchestrates use cases. Depends on `async-trait` + `tokio`, never on a concrete database or HTTP framework.
- `infrastructure` — implements the repository traits declared in `domain::repository` and the other ports in `domain::ports`. The only crate allowed to import `sqlx`, `scylla`, `redis`, `openfga`, or filesystem APIs.
- `shared` — must compile to `wasm32-unknown-unknown`. No `tokio`, no `sqlx`, no filesystem.
- `frontend` — depends ONLY on `shared`. Never reach into `domain` or `application`.
- `server` / `workers` — composition roots. They instantiate infrastructure adapters and inject them into application services.

Why: keeps business rules unit-testable in isolation, lets the frontend type-check against the same DTOs the server emits, and makes layer-tangling a compile error.

## Commands

| Task | Command |
| --- | --- |
| Build workspace (native) | `cargo build --workspace` |
| Lint + type-check | `cargo clippy --workspace --all-targets` |
| Run all tests | `cargo test --workspace` |
| Build frontend WASM | `cd crates/frontend && trunk build` |
| Run backend | `cargo run --bin server` |
| Run workers | `cargo run --bin workers` |
| Frontend dev server | `cd crates/frontend && trunk serve` |
| Bring up dep stack | `docker compose -f infra/docker-compose.yml up -d` |

`infra/` holds the Compose stack (`docker-compose.infra.yml` for the dependency services, `docker-compose.yml` for the full containerized stack), the Postgres and Scylla schema files, the OpenFGA model, and the bootstrap/backup scripts. Bring the stack up and apply schemas with `cargo make bootstrap` (idempotent). The Postgres schema is applied from `infra/postgres/10-init.sql` at bootstrap; `migrations/` is reserved for future incremental `sqlx migrate` steps.

## External services

| Service | Used for |
| --- | --- |
| PostgreSQL | Users, groups, projects, requests, tickets, attachment metadata. |
| Cassandra / ScyllaDB | Chat history (high write volume, time-series). |
| Redis | Sessions, pub/sub fan-out for WebSocket presence, rate limiting. |
| OpenFGA | Relationship-based access control derived from the org graph — never flat ACLs. |
| Local filesystem | File uploads under `STORAGE_ROOT`. No S3 / MinIO. |

## Domain summary

Six actor types — Director, HR, Group Leader, Group Sub-Leader, Member, IT. Roles are per-group and a user can hold different roles in different groups (a Director may also lead a team).

Core entities: User, Group, Project, Request, Ticket, Channel (group / general / direct), Message, Announcement.

Each entity has an explicit lifecycle:

- User: `pending` → `active` → `deactivated` (reactivatable).
- Project: `planning` → `active` → `on_hold` / `completed` / `cancelled`.
- Request: `draft` → `submitted` → `assigned` → `in_progress` → `review` → `completed`.
- Ticket: `open` → `triaged` → `assigned` → `in_progress` → `resolved` → `closed` (reopen window: 7 days).

Transitions are gated by current state AND actor role. Encode them as enums in `domain` with transition methods that return `Result<NewState, TransitionError>` — not free-form strings.

## Cross-cutting invariants

Enforce at the type or database level whenever possible:

1. Every group has exactly one leader (partial unique constraint in Postgres).
2. Every project has exactly one owner group.
3. A user holds exactly one role per group.
4. Deactivated users disappear from active queries but remain referenced in historical records (chats, request authorship, audit log).
5. Audit log entries are immutable.
6. Announcements are immutable after a 15-minute grace period.
7. Direct messages are private even from Directors.
8. A project's owner group cannot also be one of its collaborator groups.

## Conventions

- Module layout: sibling `foo.rs` + `foo/`, never `mod.rs`.
- IDs: `uuid::Uuid` v7 (time-ordered) wrapped in per-entity newtypes (`UserId`, `GroupId`, …) under `domain::ids`.
- Time: `time::OffsetDateTime` everywhere — never `chrono`, never naive timestamps.
- Errors: per-crate `thiserror` enum. `application::Error` is the boundary returned to `server` / `workers`; mapped to HTTP status in `server::error`.
- Frontend layout uses composable primitives under `crates/frontend/src/primitives/` with design tokens in `crates/frontend/src/theme/` — not ad-hoc CSS. Feature folders follow `crates/frontend/src/features/<area>/{api,components,routes}.rs`.
- Code style is enforced by `rustfmt` + `clippy`. Do not manually reformat.

## When in doubt

- New data-access trait → declare in `domain::repository`. New non-repository port (event publisher, file storage, authz client) → declare in `domain::ports`. Implement either in `infrastructure`, inject through the relevant `application` service constructor.
- New HTTP route → handler in `server::routes`, request/response DTOs in `shared::dto`, validation in `shared::validation`.
- New background job → register under `workers`, call into an `application` service.
- New permission rule → model in the OpenFGA store, check via `domain::ports::authz_client`.
