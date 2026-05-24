---
paths:
  - "crates/server/**/*.rs"
---

# Server / Axum Routing Rules

The `server` crate is a composition root: it builds adapter instances, wraps them in application services, and exposes the services over HTTP/WebSocket. Handlers stay thin — almost all behaviour lives in `application`.

## Handler shape

A handler is `async fn`, returns `Result<_, AppError>`, and never touches a database directly.

```rust
pub async fn create_project(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateProjectRequest>,
) -> Result<Json<ProjectResponse>, AppError> {
    let project = state
        .project_service
        .create(auth.user_id, body.into())
        .await?;
    Ok(Json(project.into()))
}
```

Rules:

- Extractor order: `State` first, then `AuthUser`, then `Path` / `Query`, then `Json` last. Axum extracts bodies last because they consume the request.
- Request and response types live in `shared::dto::*`. Convert via `From` / `Into` between DTOs and domain types — never expose domain types over HTTP directly.
- Return `Json<T>` for JSON bodies, `StatusCode::NO_CONTENT` for empty responses. Never raw `String` bodies.

## AppState

`AppState` is the dependency-injection seam:

```rust
#[derive(Clone)]
pub struct AppState {
    pub user_service:    Arc<UserService>,
    pub project_service: Arc<ProjectService>,
    pub ticket_service:  Arc<TicketService>,
    // ...
}
```

- One `Arc<application::*Service>` field per service.
- `AppState` is `Clone` (cheap, since fields are `Arc`).
- Built once in `app::build` from concrete infrastructure adapters.

## Router composition

One module per entity under `crates/server/src/routes/`. Each module exposes a single router builder:

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects",      post(create_project).get(list_projects))
        .route("/projects/{id}", get(get_project).patch(update_project))
}
```

- Top-level `app::build` composes the per-entity routers with `nest("/api/v1/projects", projects::router())`.
- The routes module owns its handlers — do not split handlers for a single resource across files.

## Auth

Auth lives in two pieces:

- `middleware/auth.rs` — verifies the JWT, attaches the decoded claims to request extensions.
- `extractors/auth_user.rs` — pulls the claims out, returns `AuthUser` carrying `user_id`, role bundle, and group memberships.

Rules:

- The `AuthUser` extractor is the only way handlers get the caller's identity. Do not read headers manually.
- Public endpoints opt OUT of auth by being placed on a separate, un-layered router (login, healthz).
- A failed auth check returns `AuthError`, which `AppError` converts to 401 with a structured JSON body.

## Errors → HTTP

Server-level `AppError` enum in `crates/server/src/error.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Domain(#[from] application::Error),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("validation failed: {0}")]
    Validation(String),
}
```

- Implements `IntoResponse` — that impl is the one place variant → status-code + JSON body mapping lives.
- Domain errors flow in via `From<application::Error>`. Never `match` on `application::Error` inside a handler.
- Response body has a stable `{ "code": "...", "message": "..." }` shape that the frontend deserialises into `FrontendError`.

## Middleware order

Applied bottom-up via `Router::layer()`. Desired stack, outermost to innermost:

1. `tower_http::trace::TraceLayer` — request span, latency.
2. `middleware/request_id.rs` — assigns request ID, propagates via tracing.
3. `middleware/auth.rs` — JWT verification (applied only to protected sub-routers).

Auth wraps a sub-router, not the whole `Router`. Public endpoints (login, healthz, OpenAPI doc if any) live outside the auth layer.

## WebSocket

WS handlers live in `crates/server/src/routes/chat_ws.rs`, separated from REST chat endpoints.

- Use `axum::extract::ws::WebSocketUpgrade`. The upgrade handler returns a `Response`; the per-connection task runs inside the spawned future.
- Fan-out is via redis pub/sub subscriptions held in the per-connection task. Never poll a database in the WS task.
- Per-connection state (subscribed channels, user_id) lives in the task, not in `AppState`.

## What handlers must not do

- Import `sqlx`, `scylla`, `redis`, or the OpenFGA SDK. Always go through an application service from `AppState`.
- Build SQL strings, even via macros.
- Read environment variables. Config is parsed once in `config.rs` and threaded through `app::build`.
