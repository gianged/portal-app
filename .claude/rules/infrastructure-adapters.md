---
paths:
  - "crates/infrastructure/**/*.rs"
  - "infra/postgres/*.sql"
---

# Infrastructure Adapter Rules

`infrastructure` is the only crate where IO happens. Each module implements a port from `domain::repository` (data-access traits) or `domain::ports` (event publisher, file storage, authz client) against a concrete backend. Treat backend errors as private: wrap them at the function boundary and never leak them upward.

## sqlx + PostgreSQL

Repository pattern: `struct PgUserRepo { pool: PgPool }` implementing `domain::repository::UserRepository`.

Query macros:

- `sqlx::query_as!(Row, "SELECT ...", args)` for typed reads — compile-time SQL + type check.
- `sqlx::query!("INSERT ...", args)` for writes.
- Never the runtime `sqlx::query()` / `sqlx::query_as()` (no `!`) — it silently skips type checking.

Transactions:

- Pass `&mut Transaction<'_, Postgres>` into helpers, never `PgPool`.
- Commit / rollback happens in the outermost caller (typically the application service method).

Offline mode:

- Run `cargo sqlx prepare --workspace` before committing changes that add or alter SQL.
- The resulting `.sqlx/` directory is committed.

Error wrapping at every public method:

- `sqlx::Error::RowNotFound` → `RepositoryError::NotFound`
- `sqlx::Error::Database(e)` → match on `e.kind()` (`sqlx::error::ErrorKind`):
  - `UniqueViolation` | `ForeignKeyViolation` | `CheckViolation` → `RepositoryError::Conflict(e.to_string())`
  - anything else → `RepositoryError::Backend(e.to_string())`
- any other `sqlx::Error` variant → `RepositoryError::Backend(e.to_string())`

The shortcut `e.is_unique_violation()` is fine when only that one kind is interesting. Prefer `kind()` once two or more constraint categories funnel into `Conflict` — it scales without nested `if let` chains.

`sqlx::Error` must never appear in a public function signature.

## Schema

The relational schema is database-first and lives entirely in `infra/postgres/10-init.sql` — the single source of truth. There is no incremental-migration workflow.

- Change the schema by editing `10-init.sql` directly, then reinitialize the dev DB (Postgres only runs the init script on an empty volume): `docker compose --env-file .env -f infra/docker-compose.infra.yml down -v`, then `cargo make bootstrap`, then `cargo make sqlx-prepare`.
- Keep the file's sectioned layout: tables in §5, foreign keys in §6, indexes in §7, triggers in §8.
- Column types:
  - Timestamps: `TIMESTAMPTZ NOT NULL`. Never `TIMESTAMP`.
  - Primary keys: `UUID NOT NULL`, populated with UUIDv7 from the application layer. No `SERIAL` / `BIGSERIAL`.
  - Money / fixed-precision: `NUMERIC(...)`, never `FLOAT` or `DOUBLE PRECISION`.
- Encode domain invariants in the schema when possible. Example — "exactly one leader per group":

  ```sql
  CREATE UNIQUE INDEX one_leader_per_group
      ON group_members(group_id)
      WHERE role = 'leader';
  ```

## Cassandra / ScyllaDB

The chat-history adapter lives under `crates/infrastructure/src/scylla/`.

- One shared `Arc<scylla::Session>` initialised at startup, injected into the repo.
- Prepare every statement at startup, store the `PreparedStatement` on the repo struct, reuse it. Never call `session.execute()` with a raw query string in a hot path.
- Document partition key choice in a comment above each prepared statement — partition shape is load-bearing and easy to get wrong later.
- Error wrap: the scylla query error → `RepositoryError::Backend(...)`. The exact type path lives under scylla 1.x's `errors` module — let the `use` line in the adapter carry it rather than hard-coding it here.

## Redis

The redis adapter lives under `crates/infrastructure/src/redis/`.

- Keys are constructed via dedicated functions that namespace the value:

  ```rust
  fn presence_key(user: UserId) -> String { format!("portal:presence:user:{user}") }
  ```

- Every `SET` has an explicit TTL (`EX` argument, `SETEX`, or the equivalent builder). No infinite keys.
- Pub/sub channels follow the same namespacing: `portal:pubsub:<topic>:<scope>`.
- Connection: shared `redis::aio::ConnectionManager`. Never construct a new connection per call.
- Error wrap: `redis::RedisError` → `RepositoryError::Backend(...)`.

## OpenFGA

The OpenFGA wrapper implements `domain::ports::authz_client::AuthzClient`.

- All authorization checks go through this wrapper. Application services depend on the trait, never on the OpenFGA SDK.
- Raw `openfga_*` SDK types must not appear in a function signature outside `crates/infrastructure/src/openfga/`.
- Tuple writes (relationship updates) are exposed as helper methods taking domain types — `grant_group_member(GroupId, UserId)`, not raw tuples or string user IDs.

## Cross-cutting

- Every public function returns `Result<_, RepositoryError>` (or the appropriate port error). Backend error types stay private to this crate.
- `infrastructure` depends on `domain` but never on `application`. If infra needs application logic, the design is wrong — flip the dependency by adding a port in `domain`.
- The composition root (`server` / `workers`) owns adapter construction. `infrastructure` itself never reads config or env vars.
