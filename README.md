# Portal

Internal company portal for a single organization (100–1000 users). Project tracking, work requests, IT ticketing, real-time chat, and company-wide announcements — all behind relationship-based access control. Full-stack Rust: Axum backend, Leptos WebAssembly frontend.

## Features

- Hierarchical org model: groups with one leader, multiple sub-leaders, and members. HR owns the user lifecycle.
- Project ownership at the group level; cross-group collaboration via group invites rather than per-user grants.
- Request workflows scoped to projects, with assignment, review, and approval states.
- IT ticket system with triage, priority, resolution, and a bounded reopen window.
- Real-time chat over WebSocket: group channels, an HR-broadcast general channel, and direct messages.
- Announcements with a 15-minute edit grace period and broadcast notifications.
- ReBAC authorization via OpenFGA — permissions derived from the org graph, not stored as flat ACLs.
- File uploads stored on the host machine, configurable via `STORAGE_ROOT`.

## Tech stack

| Layer | Choice |
| --- | --- |
| Language | Rust (edition 2024) |
| HTTP / WebSocket | Axum + Tokio |
| Frontend | Leptos (CSR) compiled to WebAssembly |
| Frontend build | Trunk |
| Primary database | PostgreSQL via SQLx |
| Chat history | ScyllaDB (Cassandra-compatible) |
| Sessions, pub-sub, presence, rate limit | Redis |
| Authorization | OpenFGA (ReBAC) |
| Background jobs | Apalis |
| File storage | Local filesystem |
| Observability | tracing + tracing-subscriber |
| TLS | rustls |

## Project layout

```
portal-app/
├── crates/
│   ├── domain/          Pure types, traits (ports). No async, no IO.
│   ├── application/     Business logic services. Depends only on domain.
│   ├── infrastructure/  Adapters: Postgres, Scylla, Redis, OpenFGA, local storage.
│   ├── server/          Axum HTTP + WebSocket binary.
│   ├── workers/         Apalis background-job binary.
│   ├── shared/          DTOs + validation shared by backend and frontend (native + WASM).
│   └── frontend/        Leptos SPA, built with Trunk to WebAssembly.
├── infra/               Docker Compose stack, schema files (infra/postgres/10-init.sql), OpenFGA model.
├── storage/uploads/     Local file uploads (gitignored).
├── scripts/             Dev helpers.
└── e2e/                 Full-stack end-to-end browser tests.
```

The dependency graph points inward toward `domain` on the backend; `shared` is the bridge across the WASM boundary. The compiler enforces architectural layering through crate dependency declarations.

## Prerequisites

- Rust 1.94 or newer (toolchain pinned in `rust-toolchain.toml` — `rustup` will install it automatically)
- Docker + Docker Compose
- [cargo-make](https://github.com/sagiegurari/cargo-make): `cargo install cargo-make --locked`
- [Trunk](https://trunkrs.dev): `cargo install trunk`
- [sqlx-cli](https://crates.io/crates/sqlx-cli): `cargo install sqlx-cli --no-default-features --features rustls,postgres`

## Getting started

```bash
# 1. Configure environment
cp .env.example .env

# 2. Bring up the dependency stack and apply schemas — idempotent, safe to re-run
#    (Postgres, Redis, ScyllaDB, OpenFGA). Wraps infra/scripts/init.sh.
cargo make bootstrap

# 3. Run the backend, workers, and frontend dev server together
cargo make run-all
```

`cargo make run-all` runs the server, workers, and the Trunk dev server in parallel; use `cargo make run-server` / `run-workers` / `run-frontend` to run them individually, and `cargo make up` / `cargo make down` to start and stop just the dependency stack (no re-bootstrap). The Trunk dev server proxies HTTP and WebSocket calls to the backend; see `crates/frontend/Trunk.toml`.

## Development

```bash
cargo build --workspace                       # build all native crates
cargo clippy --workspace --all-targets        # lint
cargo fmt --all                               # format
cd crates/frontend && trunk build             # build frontend WASM
```

Background workers run as a separate binary:

```bash
cargo run --bin workers
```

### Database schema

`infra/postgres/10-init.sql` is the single source of truth for the relational schema —
database-first, not ORM-managed. Postgres applies it via the docker entrypoint, but only
on an **empty** data volume, so schema changes are made in that file and then the dev
database is reinitialized:

```bash
# 1. Edit infra/postgres/10-init.sql
# 2. Wipe the dev volumes and re-apply the schema (also re-bootstraps OpenFGA + Scylla)
docker compose --env-file .env -f infra/docker-compose.infra.yml down -v
cargo make bootstrap
# 3. Regenerate the committed offline query cache against the fresh schema
cargo make sqlx-prepare
```

The infrastructure crate uses sqlx compile-time query macros; CI builds with `SQLX_OFFLINE`
against the committed `.sqlx/` cache, so commit the `.sqlx/` diff together with the schema
change.

## Testing

| Tier | Location | Tool |
| --- | --- | --- |
| Unit | `crates/*/src/` `#[cfg(test)]` | `cargo test` |
| Integration | `crates/server/tests/`, `crates/application/tests/` | `testcontainers` (real Postgres / Redis / Cassandra / OpenFGA) |
| Frontend component | `crates/frontend/tests/` | `wasm-bindgen-test` |
| End-to-end browser | `e2e/` | browser automation against a running stack |

```bash
cargo test --workspace
```

## Production build

```bash
cargo build --release --bin server --bin workers
cd crates/frontend && trunk build --release
```

Artifacts:

- `target/release/server` — HTTP / WebSocket binary
- `target/release/workers` — background job binary
- `crates/frontend/dist/` — static assets (HTML, JS glue, WASM, images)

## Deployment

CI/CD runs on GitHub Actions. Every pull request and push to `master` is gated on
formatting, Clippy (native crates plus the WASM frontend), tests, and an MSRV (1.94)
check — see `.github/workflows/ci.yml`. Each PR surfaces these as status checks; merge
once they pass. There is no automated versioning or release step.

Container images build from the repo-root `Dockerfile` (server + workers) and
`Dockerfile.frontend` (static assets served by nginx — see `infra/nginx/`). Hosting,
secrets management, and rollout strategy are still to be decided.

## Configuration

All runtime configuration is via environment variables. See `.env.example` for the full list. Highlights:

| Variable | Purpose |
| --- | --- |
| `DATABASE_URL` | PostgreSQL connection string |
| `REDIS_URL` | Redis connection string |
| `CASSANDRA_HOSTS`, `CASSANDRA_KEYSPACE` | Chat history backend |
| `OPENFGA_API_URL`, `OPENFGA_STORE_ID` | Authorization service |
| `STORAGE_ROOT` | Directory on host where uploaded files are written |
| `SERVER_HOST`, `SERVER_PORT` | Backend bind address |
| `JWT_SECRET`, `SESSION_TTL_HOURS` | Auth |
| `RUST_LOG` | Log filter (e.g. `info,portal=debug`) |

## License

MIT.
