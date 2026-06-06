use std::sync::Arc;

use anyhow::Context;
use axum::{
    Router,
    http::{HeaderValue, Method, header},
    routing::get,
};
use tower_http::cors::{AllowOrigin, CorsLayer};

use application::{
    events::EventBus,
    permissions::Permissions,
    service::{
        announcement::AnnouncementService, audit::AuditService, chat::ChatService,
        group::GroupService, notification::NotificationService, project::ProjectService,
        request::RequestService, ticket::TicketService, user::UserService,
    },
};
use domain::{
    ports::{file_storage::FileStorage, presence::Presence, rate_limit::RateLimit},
    repository::{
        AuditRepository, ChatRepository, GroupRepository, NotificationRepository,
        ProjectRepository, RequestRepository, TicketRepository, UserRepository,
    },
};
use infrastructure::{
    jobs::{ApalisAuditQueue, ApalisNotificationQueue, audit_storage, notification_storage},
    local_storage::LocalStorage,
    openfga::{self, OpenFgaAuthzClient},
    postgres::{
        PgAuditRepo, PgGroupRepo, PgNotificationRepo, PgProjectRepo, PgRequestRepo, PgTicketRepo,
        PgUserRepo, build_pool,
    },
    redis::{PresenceStore, RateLimiter, RedisEventPublisher},
    scylla::{ScyllaChatRepo, build_session},
    signed_url::SignedUrl,
};

use crate::{
    auth::TokenService, config::Config, middleware::rate_limit::RateLimits, realtime::Realtime,
    routes,
};

/// Dependency-injection seam shared by every handler. Cheap to clone — every
/// field is an `Arc`. The Redis-backed cross-cutting ports (`presence`,
/// `rate_limiter`) and the `realtime` publisher are held as trait objects so the
/// router can be exercised against in-memory fakes in tests.
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
    pub audit_service: Arc<AuditService>,
    pub presence: Arc<dyn Presence>,
    pub rate_limiter: Arc<dyn RateLimit>,
    pub rate_limits: RateLimits,
    pub storage: Arc<LocalStorage>,
    // Verifies the signed `?exp&sig` on `/files` downloads; the same signer
    // (built from `jwt_secret`) backs `LocalStorage::presign_get`.
    pub signed_url: Arc<SignedUrl>,
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
    let presence: Arc<dyn Presence> = Arc::new(
        PresenceStore::new(&cfg.redis_url)
            .await
            .context("connecting redis (presence)")?,
    );
    let rate_limiter: Arc<dyn RateLimit> = Arc::new(
        RateLimiter::new(&cfg.redis_url)
            .await
            .context("connecting redis (rate limit)")?
            .with_window(cfg.rate_limit_window_secs),
    );
    // One signer backs both presign (in the adapter) and verify (in the file
    // route); reusing `jwt_secret` avoids introducing a second deployment secret.
    let signed_url = Arc::new(SignedUrl::new(cfg.jwt_secret.as_bytes()));
    let storage = Arc::new(LocalStorage::new(
        cfg.storage_root.clone(),
        &cfg.storage_public_base,
        signed_url.clone(),
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
    let audit_jobs = ApalisAuditQueue::new(
        audit_storage(&cfg.redis_url)
            .await
            .context("connecting apalis redis (audit jobs)")?,
    );
    let events = Arc::new(EventBus::new(
        publisher.clone(),
        Arc::new(jobs),
        Arc::new(audit_jobs),
    ));
    let audit_service = Arc::new(AuditService::new(audit, perms.clone()));
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
        audit_service,
        presence,
        rate_limiter,
        rate_limits: RateLimits {
            auth: cfg.auth_rate_limit,
            api: cfg.api_rate_limit,
        },
        storage,
        signed_url,
    };

    Ok(router(state).layer(cors_layer(&cfg.cors_allowed_origins)))
}

/// Composes the route tree and middleware stack over a fully-built [`AppState`].
/// Split out from [`build`] so tests can drive the real router against in-memory
/// fakes without standing up infrastructure. CORS and connect-info are applied by
/// the caller ([`build`] / `main`), since they depend on config/runtime wiring
/// rather than the route tree.
pub fn router(state: AppState) -> Router {
    // Protected API: every route requires a valid session, then a per-user rate
    // limit. `route_layer` keeps these off unmatched paths (they 404, not 401).
    // Auth is the outer layer so it inserts `AuthUser` before the per-user limiter
    // reads it.
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
        .merge(routes::audit::router())
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::rate_limit::per_user,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::auth::require_auth,
        ));

    // Public API: login / logout carry no session yet; rate-limited per client IP.
    let public = routes::auth::public_router().route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        crate::middleware::rate_limit::per_ip,
    ));

    // File downloads authorize via the signed `?exp&sig` (verified in the
    // handler), so they sit outside both the session-auth layer and the per-user
    // limiter that the protected sub-tree carries.
    let api = public.merge(protected).merge(routes::files::router());

    Router::new()
        .route("/healthz", get(healthz))
        .nest("/api/v1", api)
        // Applied outermost-to-innermost: trace wraps request-id wraps the router
        // (whose protected sub-tree adds the per-route auth + limit layers above).
        // The caller adds CORS as the outermost layer.
        .layer(axum::middleware::from_fn(
            crate::middleware::request_id::propagate,
        ))
        .layer(crate::middleware::trace::layer())
        .with_state(state)
}

/// Builds the credentialed CORS layer from the configured origins. Invalid origin
/// strings are dropped with a warning rather than failing startup. Credentialed
/// CORS forbids a wildcard origin, so the allow-list is always explicit.
pub fn cors_layer(origins: &[String]) -> CorsLayer {
    let parsed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|o| match o.parse::<HeaderValue>() {
            Ok(value) => Some(value),
            Err(_) => {
                tracing::warn!(origin = %o, "ignoring invalid CORS origin");
                None
            }
        })
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(parsed))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE])
}

async fn healthz() -> &'static str {
    "ok"
}
