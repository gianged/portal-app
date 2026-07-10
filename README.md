# Portal

Internal company portal for a single organization (100–1000 users): project tracking, work requests, IT ticketing, attendance & leave management, real-time chat, and company-wide announcements — all behind relationship-based access control.

Full-stack Rust — an Axum HTTP/WebSocket backend, a Leptos WebAssembly frontend, and ReBAC authorization via OpenFGA, backed by PostgreSQL, ScyllaDB, and Redis. This README is written for operators running and deploying the stack.

## Contents

- [What it does](#what-it-does)
- [Tech stack](#tech-stack)
- [Architecture](#architecture)
- [Prerequisites](#prerequisites)
- [Quick start (local)](#quick-start-local)
- [Configuration](#configuration)
- [Deployment](#deployment)
- [Ports](#ports)
- [Operations](#operations)
- [Database schema](#database-schema)
- [Testing](#testing)
- [License](#license)

## What it does

- **Org model** — groups with one leader, multiple sub-leaders, and members; HR owns the user lifecycle.
- **Projects** — owned at the group level, with cross-group collaboration via group invites.
- **Requests** — work-request workflows scoped to projects, with assignment, review, and approval states.
- **Tickets** — IT ticketing with triage, priority, resolution, and a bounded reopen window.
- **Attendance & leave** — daily work reports with leader review, day-off requests (annual, sick, unpaid, remote), overtime, and flexible working hours enforcing core hours plus monthly reconciliation.
- **Leave & policy** — yearly leave grants with FIFO carryover and expiry, a company holiday calendar, and HR-configured attendance policy; monthly / yearly / per-staff reports export to PDF.
- **Chat** — real-time over WebSocket: group channels, an HR-broadcast general channel, and direct messages.
- **Announcements** — company-wide, with a 15-minute edit grace period and broadcast notifications.
- **Authorization** — ReBAC via OpenFGA; permissions are derived from the org graph, not stored as flat ACLs.
- **File uploads** — stored on the host filesystem under `STORAGE_ROOT`, served via signed URLs.
- **External read API** — `/api/ext/v1`, a read-only surface for scripts and reporting tools, authenticated with service-account keys (see [Operations](#external-read-api)).

## Tech stack

| Layer | Choice |
| --- | --- |
| Language | Rust (edition 2024, MSRV 1.94) |
| HTTP / WebSocket | Axum + Tokio |
| Frontend | Leptos (CSR) compiled to WebAssembly, built with Trunk |
| Primary database | PostgreSQL via SQLx |
| Chat history | ScyllaDB (Cassandra-compatible) |
| Sessions, pub-sub, presence, rate limit | Redis |
| Authorization | OpenFGA (ReBAC) |
| Background jobs | Apalis |
| Internal RPC | gRPC (tonic + prost) between server and workers |
| File storage | Local filesystem |
| Observability | tracing + tracing-subscriber |
| TLS | rustls |

## Architecture

```
portal-app/
├── crates/
│   ├── domain/          Pure types, traits (ports). No async, no IO.
│   ├── application/     Business logic services. Depends only on domain.
│   ├── infrastructure/  Adapters: Postgres, Scylla, Redis, OpenFGA, local storage.
│   ├── proto/           Internal gRPC contracts (tonic/prost). Native-only.
│   ├── server/          Axum HTTP + WebSocket + internal gRPC query-plane binary.
│   ├── workers/         Apalis background-job binary + internal gRPC job ingest.
│   ├── shared/          DTOs + validation shared by backend and frontend (native + WASM).
│   └── frontend/        Leptos SPA, built with Trunk to WebAssembly.
├── infra/               Docker Compose stack, schema files, OpenFGA model, nginx, scripts.
├── docs/api/            Public contract for the external read API.
├── loadtest/            Go load-test harness (login, API mix, WebSocket chat).
├── storage/uploads/     Local file uploads (gitignored).
└── e2e/                 Full-stack end-to-end browser tests.
```

Two runtime processes — **server** (HTTP/WebSocket) and **workers** (background jobs) — share the application and infrastructure crates and connect to the same backing stores. They also talk to each other over an internal, token-gated gRPC plane: the server dispatches jobs to the workers' gRPC ingest first, falling back to a direct Apalis enqueue and then to a Redis job spool, with each hop circuit-breaker-gated.

## Prerequisites

- Rust 1.94+ (pinned in `rust-toolchain.toml` — `rustup` installs it automatically)
- Docker + Docker Compose
- `protoc` (Protocol Buffers compiler) — `crates/proto`'s build script shells out to it (`apt install protobuf-compiler` / `winget install protobuf` / `brew install protobuf`)
- [cargo-make](https://github.com/sagiegurari/cargo-make): `cargo install cargo-make --locked`
- [Trunk](https://trunkrs.dev): `cargo install trunk` *(only for host-run frontend)*
- [sqlx-cli](https://crates.io/crates/sqlx-cli): `cargo install sqlx-cli --no-default-features --features rustls,postgres` *(only for schema changes)*

## Quick start (local)

Run the dependency stack in containers, the app on the host (best for development — breakpoints, hot reload):

```bash
# 1. Configure environment
cp .env.example .env

# 2. Bring up the dependency stack and apply schemas (idempotent, safe to re-run)
cargo make bootstrap

# 3. Run server + workers + frontend dev server together
cargo make run-all
```

The frontend dev server (Trunk) serves on **8081** and proxies API/WebSocket calls to the backend (see `crates/frontend/Trunk.toml`). Run pieces individually with `cargo make run-server` / `run-workers` / `run-frontend`, and start/stop just the dependency stack with `cargo make infra-up` / `cargo make infra-down` (no re-bootstrap).

Optionally load demo data (~100 employees + org + sample activity, re-runnable):

```bash
cargo make seed
```

## Configuration

All runtime config is via environment variables; `.env.example` is the full, commented list. Copy it to `.env` and edit. The defaults are wired for local dev — the items below **must change for production**.

### Secrets (regenerate for production)

Generate each with `openssl rand -hex 32`. These are placeholders in `.env.example` and must not ship as-is.

| Variable | Purpose |
| --- | --- |
| `JWT_SECRET` | Signs session tokens (min 32 bytes). |
| `STORAGE_SIGNING_SECRET` | Signs presigned file-download URLs (distinct from `JWT_SECRET`). |
| `INTERNAL_GRPC_TOKEN` | Shared bearer token gating the internal gRPC plane (min 32 bytes). Required at boot for both server and workers. |
| `REDIS_PASSWORD` | Redis auth — also embedded in `REDIS_URL`. Never expose Redis unauthenticated. |
| `OPENFGA_BEARER_TOKEN` | OpenFGA API auth — required in production (see below). |
| `POSTGRES_PASSWORD` | Database password (containerized Postgres). |

### Core connection settings

| Variable | Example | Purpose |
| --- | --- | --- |
| `DATABASE_URL` | `postgres://portal:portal@localhost:5432/portal` | PostgreSQL connection string. |
| `REDIS_URL` | `redis://:<pw>@localhost:6379` | Redis connection (sessions, pub-sub, presence, rate limit). |
| `SCYLLA_HOSTS`, `SCYLLA_KEYSPACE` | `localhost:9042`, `portal_chat` | Chat-history backend. |
| `OPENFGA_API_URL`, `OPENFGA_STORE_ID` | `http://localhost:8088` | Authorization service. `STORE_ID` is populated by bootstrap. |
| `SERVER_HOST`, `SERVER_PORT` | `0.0.0.0`, `8090` | Backend bind address. |

### Internal gRPC plane (server ↔ workers)

| Variable | Example | Purpose |
| --- | --- | --- |
| `SERVER_GRPC_ADDR` | `0.0.0.0:50051` | Server query-plane bind address. |
| `WORKERS_GRPC_ADDR` | `0.0.0.0:50052` | Workers job-ingest bind address. |
| `WORKERS_GRPC_URL` | `http://127.0.0.1:50052` | Where the server dials the workers' ingest (primary job-dispatch hop). |
| `INTERNAL_GRPC_TOKEN` | *(secret)* | Shared bearer token for both planes — see secrets above. |

The plane is internal-only: keep both ports firewalled from anything that is not the server/workers pair.

### External read API (`/api/ext/v1`)

| Variable | Default | Purpose |
| --- | --- | --- |
| `EXT_RATE_LIMIT` | `60` | Per-service-account-key request ceiling per rate-limit window. |
| `EXT_IP_RATE_LIMIT` | `120` | Per-IP ceiling applied before key auth (several keys may share one host). |

### Storage, auth, and behavior

| Variable | Example | Purpose |
| --- | --- | --- |
| `STORAGE_ROOT` | `./storage/uploads` | Directory where uploads are written — use an absolute path on a persistent volume in production. |
| `STORAGE_PUBLIC_BASE` | `http://localhost:8090/api/v1` | Public base for signed URLs — **must include `/api/v1`**. |
| `SESSION_TTL_HOURS` | `24` | Session lifetime. |
| `HEALTH_PROBE_INTERVAL_SECS` | `5` | How often backends are probed (drives circuit breakers and `/readyz`). |
| `RUST_LOG` | `info,portal=debug` | Log filter. |

### Network access gate

An IP allowlist middleware runs before auth and any handler. With `IP_ALLOWLIST_ENABLED=true` (default) only clients matching `IP_ALLOWLIST` reach the API — others get `403`. When `IP_ALLOWLIST` is unset it defaults to loopback + private ranges, so LAN and VPN clients pass; set your real networks in production. Behind a reverse proxy, set `TRUSTED_PROXIES` to the proxy's addresses (CIDR, comma-separated): when the immediate peer matches, the client is taken from `X-Forwarded-For` (rightmost non-trusted hop); from any other peer the header is ignored, since it is spoofable. The gate fails closed: a request with no resolvable peer IP — or a trusted proxy forwarding a missing/malformed chain — is rejected.

### Production-only authorization hardening

- `OPENFGA_ALLOW_NO_AUTH` defaults to `false`, which makes `OPENFGA_BEARER_TOKEN` mandatory. Setting it to `true` (OpenFGA without auth) is a **local-dev-only** escape hatch — never in production.
- Set `OPENFGA_DATASTORE_SSLMODE=require` (TLS to its Postgres datastore).

### Email (workers, optional)

In-app notifications are always on; email is opt-in. Set `EMAIL_ENABLED=true` and the `SMTP_*` settings (`SMTP_HOST`, `SMTP_PORT`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `SMTP_FROM`, `SMTP_TLS`). `PORTAL_BASE_URL` is the public frontend origin used to build links in emails.

## Deployment

Two Compose files share the same data volumes, so you can switch between them without losing data:

- **`infra/docker-compose.infra.yml`** (`cargo make infra-up` / `infra-down`) — dependency services only (Postgres, ScyllaDB, Redis, OpenFGA). Used by the host-run quick-start flow above. Publishes every backing store to the host.
- **`infra/docker-compose.yml`** (`cargo make up` / `down`) — the full containerized stack: the four dependencies **plus** `server`, `workers`, and `frontend`. OpenFGA is not published to the host here. The server container is health-checked on `/healthz`.

```bash
cargo make up      # full containerized stack (deps + server + workers + frontend)
cargo make down    # stop it, keeping data volumes
```

### Images

- **`Dockerfile`** (repo root) — builds the Rust binaries. Multi-target: `runtime` (debian-slim, runs one binary chosen via `--build-arg BINARY_NAME=server|workers`, exposes 8090) and `dev` (rust + cargo-watch for live rebuild). The same image serves both `server` and `workers`.
- **`Dockerfile.frontend`** — builds the WASM frontend. `runtime` target runs `trunk build --release` and serves `dist/` from nginx on port 80; `dev` target runs `trunk serve` on 8081.

### nginx (frontend container)

`infra/nginx/nginx.conf` serves the SPA on port 80 and:

- proxies `/api/` → `server:8090` (REST, auth, files),
- proxies `/ws/` → `server:8090` with WebSocket upgrade and 24h read timeout,
- falls back unmatched routes to `index.html` (client-side routing),
- caches fingerprinted static assets for 30 days,
- sets `nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer`, and a per-request nonce CSP.

## Ports

| Service | Container port | Host port (env override) | Notes |
| --- | --- | --- | --- |
| Backend server | 8090 | `8090` (`SERVER_HOST_PORT`) | REST + WebSocket + files + health |
| Server gRPC | 50051 | `50051` (`SERVER_GRPC_HOST_PORT`) | Internal query plane — token-gated, keep firewalled |
| Workers gRPC | 50052 | — (not published) | Internal job ingest; host-run dev binds 50052 directly |
| Frontend (nginx) | 80 | `80` (`FRONTEND_HOST_PORT`) | Full-stack compose only |
| Frontend dev (Trunk) | 8081 | `8081` (`FRONTEND_DEV_PORT`) | Host-run quick start only |
| PostgreSQL | 5432 | `5432` (`POSTGRES_HOST_PORT`) | |
| Redis | 6379 | `6379` (`REDIS_HOST_PORT`) | |
| ScyllaDB | 9042 | `9042` (`SCYLLA_HOST_PORT`) | CQL |
| OpenFGA HTTP | 8080 | `8088` (`OPENFGA_HTTP_HOST_PORT`) | OpenFGA's own container port; host 8088 keeps it distinct |
| OpenFGA gRPC | 8081 | `8089` (`OPENFGA_GRPC_HOST_PORT`) | |

## Operations

### Health endpoints

| Endpoint | Meaning | Behavior |
| --- | --- | --- |
| `GET /healthz` | Liveness | Always `200 "ok"`, dependency-free. Use for orchestrator liveness probes. |
| `GET /readyz` | Readiness | Per-backend JSON status; returns `503` if any backend is down, `200` otherwise. Use for load-balancer drain. |

Readiness is driven by per-backend circuit breakers, probed every `HEALTH_PROBE_INTERVAL_SECS`.

### External read API

`/api/ext/v1` is a strictly read-only JSON API for scripts and reporting tools (projects, work requests, company reports). Authentication is via service-account keys (`pak_...`), created by a Director or HR at `POST /api/v1/service-accounts` — the key is shown once at creation and revocable at any time. Scopes map to endpoint groups; requests are rate-limited per key (`EXT_RATE_LIMIT`) and per IP (`EXT_IP_RATE_LIMIT`), and the IP allowlist applies before authentication. The full public contract lives in [`docs/api/external-api.md`](docs/api/external-api.md).

### Bootstrap

`cargo make bootstrap` (wraps `infra/scripts/init.sh`) is idempotent and safe to re-run. It brings the stores up, applies the Postgres schema (on an empty volume), migrates the OpenFGA datastore, applies the Scylla schema, and starts OpenFGA. A default admin user is created by the schema.

### Backups

On-demand, not automated by the stack:

```bash
cargo make backup
```

Dumps Postgres (app + OpenFGA databases) and snapshots the Scylla keyspace into `BACKUP_DIR` (default `./backups`), pruning archives older than `BACKUP_KEEP_DAYS` (default 7). Schedule via cron/systemd timer in production.

### Database schema

`infra/postgres/10-init.sql` is the single source of truth for the relational schema — database-first, not ORM-managed. Postgres applies it via the docker entrypoint only on an **empty** volume, so schema changes mean editing that file and reinitializing the dev database:

```bash
docker compose --env-file .env -f infra/docker-compose.infra.yml down -v   # wipe volumes
cargo make bootstrap                                                        # re-apply schema
cargo make sqlx-prepare                                                     # regenerate .sqlx cache
```

CI builds with `SQLX_OFFLINE` against the committed `.sqlx/` cache — commit the `.sqlx/` diff together with the schema change.

## Testing

| Tier | Location | Tool |
| --- | --- | --- |
| Unit | `crates/*/src/` `#[cfg(test)]` | `cargo test` |
| Integration | `crates/server/tests/`, `crates/application/tests/` | `testcontainers` (real Postgres / Redis / Scylla / OpenFGA) |
| Frontend component | `crates/frontend/tests/` | `wasm-bindgen-test` |
| End-to-end browser | `e2e/` | browser automation against a running stack |
| Load | `loadtest/` | Go harness — login, API mix, WebSocket chat scenarios (see its README) |

```bash
cargo make test            # whole workspace
cargo make clippy          # lint (deny warnings)
```

CI runs on GitHub Actions; every PR and push to `master` is gated on formatting, Clippy (native + WASM frontend), tests, and an MSRV (1.94) check.

## License

MIT.
