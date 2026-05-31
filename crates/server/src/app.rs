use std::sync::Arc;

use anyhow::Context;
use axum::{Router, routing::get};

use application::{
    events::EventBus,
    permissions::Permissions,
    service::{
        announcement::AnnouncementService, chat::ChatService, group::GroupService,
        notification::NotificationService, project::ProjectService, request::RequestService,
        ticket::TicketService, user::UserService,
    },
};
use domain::{
    ports::file_storage::FileStorage,
    repository::{
        AuditRepository, ChatRepository, GroupRepository, NotificationRepository,
        ProjectRepository, RequestRepository, TicketRepository, UserRepository,
    },
};
use infrastructure::{
    jobs::{ApalisNotificationQueue, notification_storage},
    local_storage::LocalStorage,
    openfga::{self, OpenFgaAuthzClient},
    postgres::{
        PgAuditRepo, PgGroupRepo, PgNotificationRepo, PgProjectRepo, PgRequestRepo, PgTicketRepo,
        PgUserRepo, build_pool,
    },
    redis::{PresenceStore, RateLimiter, RedisEventPublisher},
    scylla::{ScyllaChatRepo, build_session},
};

use crate::{auth::TokenService, config::Config, realtime::Realtime, routes};

/// Dependency-injection seam shared by every handler. Cheap to clone — every
/// field is an `Arc`.
///
/// `dead_code` is allowed until the HTTP routes/handlers (plus the rate-limit
/// middleware, chat WebSocket, and upload handler) that read these fields land;
/// constructing the full graph here proves the composition wires up.
#[allow(dead_code)]
#[derive(Clone)]
pub struct AppState {
    pub user: Arc<UserService>,
    pub group: Arc<GroupService>,
    pub project: Arc<ProjectService>,
    pub request: Arc<RequestService>,
    pub ticket: Arc<TicketService>,
    pub chat: Arc<ChatService>,
    pub announcement: Arc<AnnouncementService>,
    pub notification: Arc<NotificationService>,
    // Session-cookie tokens + the real-time pub/sub handle, consumed by the auth
    // middleware and the chat WebSocket respectively.
    pub token: Arc<TokenService>,
    pub realtime: Realtime,
    // Adapters not yet behind an application service; wired for the handlers,
    // middleware, and audit wiring that land with the HTTP routes.
    pub audit: Arc<dyn AuditRepository>,
    pub presence: Arc<PresenceStore>,
    pub rate_limiter: Arc<RateLimiter>,
    pub storage: Arc<LocalStorage>,
}

/// Builds every infrastructure adapter, assembles the application services, and
/// returns the HTTP router. `OpenFGA` is initialised here (get-or-create store +
/// model), so no external bootstrap step is required.
// Composition root: one linear list of adapter + service constructors; splitting
// it would only scatter the wiring across helpers.
#[allow(clippy::too_many_lines)]
pub async fn build(cfg: &Config) -> anyhow::Result<Router> {
    // Backends.
    let pool = build_pool(&cfg.database_url, cfg.pg_max_connections)
        .await
        .context("building postgres pool")?;
    let session = build_session(&cfg.scylla_hosts, &cfg.scylla_keyspace)
        .await
        .context("building scylla session")?;
    let publisher = Arc::new(
        RedisEventPublisher::new(&cfg.redis_url)
            .await
            .context("connecting redis (events)")?,
    );
    let presence = PresenceStore::new(&cfg.redis_url)
        .await
        .context("connecting redis (presence)")?;
    let rate_limiter = RateLimiter::new(&cfg.redis_url)
        .await
        .context("connecting redis (rate limit)")?;
    let storage = Arc::new(LocalStorage::new(
        cfg.storage_root.clone(),
        &cfg.storage_public_base,
    ));

    // Repositories (as port trait objects).
    let users: Arc<dyn UserRepository> = Arc::new(PgUserRepo::new(pool.clone()));
    let groups: Arc<dyn GroupRepository> = Arc::new(PgGroupRepo::new(pool.clone()));
    let projects: Arc<dyn ProjectRepository> = Arc::new(PgProjectRepo::new(pool.clone()));
    let requests: Arc<dyn RequestRepository> = Arc::new(PgRequestRepo::new(pool.clone()));
    let tickets: Arc<dyn TicketRepository> = Arc::new(PgTicketRepo::new(pool.clone()));
    let notifications: Arc<dyn NotificationRepository> =
        Arc::new(PgNotificationRepo::new(pool.clone()));
    let audit: Arc<dyn AuditRepository> = Arc::new(PgAuditRepo::new(pool.clone()));
    let chats: Arc<dyn ChatRepository> = Arc::new(
        ScyllaChatRepo::new(session)
            .await
            .context("preparing scylla statements")?,
    );

    // OpenFGA: resolve store + authorization model at startup.
    let model_json = tokio::fs::read_to_string(&cfg.openfga_model_path)
        .await
        .with_context(|| {
            format!(
                "reading openfga model from {}",
                cfg.openfga_model_path.display()
            )
        })?;
    let fga_config = openfga::resolve_config(
        &cfg.openfga_api_url,
        "portal",
        &model_json,
        cfg.openfga_bearer_token.clone(),
    )
    .await
    .context("resolving openfga store/model")?;
    let authz = OpenFgaAuthzClient::new(fga_config).context("building openfga client")?;

    // Cross-cutting wrappers. `Permissions` reads users/groups to resolve roles,
    // so it takes those repositories alongside the authz client.
    let perms = Arc::new(Permissions::new(
        users.clone(),
        groups.clone(),
        Arc::new(authz),
    ));

    // Idempotent org bootstrap: company singleton tuples + general channel.
    application::bootstrap::seed_company(chats.as_ref(), perms.as_ref())
        .await
        .context("seeding company singleton")?;
    let jobs = ApalisNotificationQueue::new(
        notification_storage(&cfg.redis_url)
            .await
            .context("connecting apalis redis (jobs)")?,
    );
    let events = Arc::new(EventBus::new(publisher.clone(), Arc::new(jobs)));
    let storage_port: Arc<dyn FileStorage> = storage.clone();

    // Cookie-session tokens and the real-time pub/sub handle (chat WebSocket).
    let token = Arc::new(TokenService::new(
        &cfg.jwt_secret,
        cfg.session_ttl_secs,
        cfg.cookie_secure,
    ));
    let realtime = Realtime::new(publisher, cfg.redis_url.clone());

    // Application services, each built per its own constructor.
    let state = AppState {
        user: Arc::new(UserService::new(
            users.clone(),
            groups.clone(),
            requests.clone(),
            chats.clone(),
            perms.clone(),
            events.clone(),
        )),
        group: Arc::new(GroupService::new(
            groups.clone(),
            projects.clone(),
            chats.clone(),
            perms.clone(),
            events.clone(),
        )),
        project: Arc::new(ProjectService::new(
            projects.clone(),
            requests.clone(),
            perms.clone(),
            events.clone(),
        )),
        request: Arc::new(RequestService::new(
            requests.clone(),
            projects.clone(),
            groups.clone(),
            storage_port,
            perms.clone(),
            events.clone(),
        )),
        ticket: Arc::new(TicketService::new(
            tickets.clone(),
            perms.clone(),
            events.clone(),
        )),
        chat: Arc::new(ChatService::new(
            chats.clone(),
            users.clone(),
            perms.clone(),
            events.clone(),
        )),
        announcement: Arc::new(AnnouncementService::new(
            chats.clone(),
            perms.clone(),
            events.clone(),
        )),
        notification: Arc::new(NotificationService::new(
            notifications.clone(),
            perms.clone(),
        )),
        token,
        realtime,
        audit,
        presence: Arc::new(presence),
        rate_limiter: Arc::new(rate_limiter),
        storage,
    };

    // Protected API: every route requires a valid session. `route_layer` keeps
    // the auth check off unmatched paths (they 404 rather than 401).
    let protected = routes::auth::me_router()
        .merge(routes::users::router())
        .merge(routes::groups::router())
        .merge(routes::projects::router())
        .merge(routes::requests::router())
        .merge(routes::tickets::router())
        .merge(routes::chat::router())
        .merge(routes::chat_ws::router())
        .merge(routes::announcements::router())
        .merge(routes::notifications::router())
        .merge(routes::files::router())
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::auth::require_auth,
        ));

    // Public API: login / logout carry no session yet.
    let api = routes::auth::public_router().merge(protected);

    Ok(Router::new()
        .route("/healthz", get(healthz))
        .nest("/api/v1", api)
        // Applied outermost-to-innermost: trace wraps request-id wraps the
        // router (whose protected sub-tree adds the per-route auth layer above).
        .layer(axum::middleware::from_fn(
            crate::middleware::request_id::propagate,
        ))
        .layer(crate::middleware::trace::layer())
        .with_state(state))
}

async fn healthz() -> &'static str {
    "ok"
}
