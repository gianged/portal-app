use std::{
    any::Any,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use axum::{
    Json, Router,
    error_handling::HandleErrorLayer,
    extract::{DefaultBodyLimit, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing,
};
use tokio::{fs, sync::oneshot, task::JoinHandle};
use tower::{
    BoxError, ServiceBuilder, limit::GlobalConcurrencyLimitLayer, load_shed::LoadShedLayer,
};
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::CompressionLayer,
    cors::{AllowOrigin, CorsLayer},
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
};

use application::{
    bootstrap,
    events::EventBus,
    permissions::Permissions,
    resilience::{self, DispatchQueue, HealthRegistry, retry},
    service::{
        AnnouncementService, AuditService, ChatIngest, ChatIngestConfig, ChatService,
        CommentService, DailyReportService, DayOffService, ExtReadService, FlexHoursService,
        GroupService, HolidayService, LeaveBalanceService, NotificationService, OvertimeService,
        PolicyProvider, PolicyService, ProjectService, ReadPlaneService, ReportService,
        RequestService, ServiceAccountService, TicketService, UserService,
    },
};
use domain::{
    health::{BackendId, HealthStatus},
    ports::{
        event_subscriber::EventSubscriber, file_storage::FileStorage, health::HealthCheck,
        job_queue::JobQueue, presence::Presence, rate_limit::RateLimit,
        report_renderer::ReportRenderer, spool::Spool, token_revocation::TokenRevocation,
    },
    repository::{
        AuditRepository, ChatAttachmentRepository, ChatRepository, CommentRepository,
        DailyReportRepository, DayOffRepository, FlexHoursRepository, GroupRepository,
        HolidayRepository, LeaveBalanceRepository, NotificationRepository, OvertimeRepository,
        PolicyRepository, ProjectRepository, ReportArchiveRepository, ReportStatsRepository,
        RequestRepository, ServiceAccountRepository, TicketRepository, UserRepository,
    },
};
use infrastructure::{
    grpc_jobs::GrpcJobQueue,
    health::{
        OpenFgaHealthCheck, PgHealthCheck, RedisHealthCheck, ScyllaHealthCheck,
        WorkersGrpcHealthCheck,
    },
    jobs::{self, ApalisAuditQueue, ApalisNotificationQueue},
    local_storage::LocalStorage,
    openfga::{self, OpenFgaAuthzClient},
    postgres::{
        self, PgAuditRepo, PgChatAttachmentRepo, PgCommentRepo, PgDailyReportRepo, PgDayOffRepo,
        PgFlexRepo, PgGroupRepo, PgHolidayRepo, PgLeaveBalanceRepo, PgNotificationRepo,
        PgOvertimeRepo, PgPolicyRepo, PgProjectRepo, PgReportingRepo, PgRequestRepo,
        PgServiceAccountRepo, PgTicketRepo, PgUserRepo,
    },
    redis::{
        PresenceStore, RateLimiter, RedisEventPublisher, RedisEventSubscriber, RedisSpool,
        RedisTokenRevocation,
    },
    report::PrintPdfReportRenderer,
    scylla::{self, ScyllaChatRepo},
    signed_url::SignedUrl,
    telemetry,
};
use shared::dto::{
    common::{ApiError, ErrorCode},
    health::{BackendHealth, BackendStatus, ReadinessResponse},
};

use crate::{
    auth::TokenService,
    config::Config,
    grpc::{GrpcPlane, query::QueryService},
    middleware::{
        auth,
        ip_allowlist::{self, IpAllowlist},
        rate_limit::{self, RateLimits},
        request_id, service_account, trace,
    },
    realtime::Realtime,
    resolve,
    // `routes::auth` stays path-qualified at call sites: `auth` here names the
    // middleware module.
    routes::{
        self, announcements, audit, chat, chat_ws, daily_reports, day_off, ext, files, flex_hours,
        groups, holidays, leave_balance, notifications, overtime, policy, projects, reports,
        requests, service_accounts, tickets, users,
    },
};

/// Dependency-injection seam shared by every handler; cheap to clone since every
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
    // Write-behind buffer in front of chat persistence; the WS SendMessage path
    // enqueues here instead of calling `chat.post_message` inline.
    pub chat_ingest: Arc<ChatIngest>,
    pub comment: Arc<CommentService>,
    pub announcement: Arc<AnnouncementService>,
    pub notification: Arc<NotificationService>,
    pub report: Arc<ReportService>,
    // Tunable attendance limits, cached and swapped on update. Other attendance
    // services read the same `PolicyProvider` this service wraps.
    pub policy: Arc<PolicyService>,
    // Daily reports: staff describe their day; leaders review per group.
    pub daily_report: Arc<DailyReportService>,
    // HR-maintained public-holiday calendar.
    pub holiday: Arc<HolidayService>,
    // Leave grants + ledger; consumed/refunded by day-off, swept for expiry.
    pub leave: Arc<LeaveBalanceService>,
    // Leave requests with leader/HR approval.
    pub day_off: Arc<DayOffService>,
    // Overtime requests with leader/HR approval, capped monthly by policy.
    pub overtime: Arc<OvertimeService>,
    // Per-day flexible-hours requests with leader approval, settled monthly.
    pub flex: Arc<FlexHoursService>,
    // Admin-managed API keys for the external read surface.
    pub service_accounts: Arc<ServiceAccountService>,
    // Scope-gated read-only queries backing /api/ext/v1.
    pub ext_read: Arc<ExtReadService>,
    // Director/HR gate for the report endpoints (resolved per request).
    pub perms: Arc<Permissions>,
    // Session-cookie tokens + the real-time pub/sub handle, consumed by the auth
    // middleware and the chat WebSocket respectively.
    pub token: Arc<TokenService>,
    // Server-side token revocation (logout denylist + per-user version), checked by auth middleware.
    pub revocation: Arc<dyn TokenRevocation>,
    pub realtime: Realtime,
    // Short-TTL user-summary cache for the WS fan-out path.
    pub summary_cache: Arc<resolve::SummaryCache>,
    pub audit: Arc<AuditService>,
    pub presence: Arc<dyn Presence>,
    pub rate_limiter: Arc<dyn RateLimit>,
    pub rate_limits: RateLimits,
    // Network gate: allowlisted source CIDRs + enable flag, checked by the
    // ip_allowlist middleware before auth.
    pub ip_allowlist: IpAllowlist,
    pub storage: Arc<LocalStorage>,
    // Verifies the signed `?exp&sig` on `/files` downloads; the same signer
    // (built from `STORAGE_SIGNING_SECRET`) backs `LocalStorage::presign_get`.
    pub signed_url: Arc<SignedUrl>,
    // Per-backend circuit breakers + health snapshot, read by `/readyz`.
    pub health: Arc<HealthRegistry>,
}

/// Owns the chat ingest drain task so the serving loop can flush its buffered
/// tail before exit. [`Self::shutdown`] signals the loop and waits for it.
pub struct IngestShutdown {
    trigger: oneshot::Sender<()>,
    drain: JoinHandle<()>,
}

impl IngestShutdown {
    /// Signals the chat drain loop to flush its tail, then awaits it. Errors
    /// (loop already gone) are ignored: there is nothing left to drain.
    pub async fn shutdown(self) {
        let _ = self.trigger.send(());
        let _ = self.drain.await;
    }
}

/// Builds every infrastructure adapter, assembles the application services, and
/// returns the HTTP router, an [`IngestShutdown`] handle for the chat drain
/// task, and the internal [`GrpcPlane`] for `run` to serve. `OpenFGA` is
/// initialised here (get-or-create store + model), so no external bootstrap
/// step is required.
///
/// # Panics
/// Panics only if the workers-gRPC breaker is missing from the health registry,
/// which cannot happen: the registry is built from `BackendId::ALL` just above.
#[allow(clippy::too_many_lines)]
pub async fn build(cfg: &Config) -> anyhow::Result<(Router, IngestShutdown, GrpcPlane)> {
    // Backends. Every connect below retries with backoff against this shared
    // deadline, so a binary started before its infra waits instead of failing.
    let deadline = Instant::now() + cfg.startup_timeout;
    let pool = retry::until_deadline("postgres", deadline, || {
        postgres::build_pool(&cfg.database_url, cfg.pg_max_connections)
    })
    .await
    .context("building postgres pool")?;
    let session = retry::until_deadline("scylla", deadline, || {
        scylla::build_session(&cfg.scylla_hosts, &cfg.scylla_keyspace)
    })
    .await
    .context("building scylla session")?;
    let publisher = Arc::new(
        retry::until_deadline("redis (events)", deadline, || {
            RedisEventPublisher::new(&cfg.redis_url)
        })
        .await
        .context("connecting redis (events)")?,
    );
    let subscriber: Arc<dyn EventSubscriber> = Arc::new(
        RedisEventSubscriber::new(&cfg.redis_url).context("building redis event subscriber")?,
    );
    let presence: Arc<dyn Presence> = Arc::new(
        retry::until_deadline("redis (presence)", deadline, || {
            PresenceStore::new(&cfg.redis_url)
        })
        .await
        .context("connecting redis (presence)")?,
    );
    let rate_limiter: Arc<dyn RateLimit> = Arc::new(
        retry::until_deadline("redis (rate limit)", deadline, || {
            RateLimiter::new(&cfg.redis_url)
        })
        .await
        .context("connecting redis (rate limit)")?
        .with_window(cfg.rate_limit_window_secs),
    );
    // Version keys outlive session TTL by 2x and refresh on touch, so they can't lapse under a live token.
    let revocation: Arc<dyn TokenRevocation> = Arc::new(
        retry::until_deadline("redis (token revocation)", deadline, || {
            RedisTokenRevocation::new(&cfg.redis_url, cfg.session_ttl_secs * 2)
        })
        .await
        .context("connecting redis (token revocation)")?,
    );
    // One signer for presign + verify; key is dedicated (never the JWT secret) so tokens and links can't forge each other.
    let signed_url = Arc::new(SignedUrl::new(cfg.storage_signing_secret.as_bytes()));
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
    let comments: Arc<dyn CommentRepository> = Arc::new(PgCommentRepo::new(pool.clone()));
    let chat_attachments: Arc<dyn ChatAttachmentRepository> =
        Arc::new(PgChatAttachmentRepo::new(pool.clone()));
    // Clone the session for the health probe before the repo takes ownership.
    let scylla_health: Arc<dyn HealthCheck> = Arc::new(ScyllaHealthCheck::new(session.clone()));
    let chats: Arc<dyn ChatRepository> = Arc::new(
        retry::until_deadline("scylla", deadline, || ScyllaChatRepo::new(session.clone()))
            .await
            .context("preparing scylla statements")?,
    );

    // OpenFGA: resolve store + authorization model at startup.
    let model_json = fs::read_to_string(&cfg.openfga_model_path)
        .await
        .with_context(|| {
            format!(
                "reading openfga model from {}",
                cfg.openfga_model_path.display()
            )
        })?;
    let fga_config = retry::until_deadline("openfga", deadline, || {
        openfga::resolve_config(
            &cfg.openfga_api_url,
            "portal",
            &model_json,
            cfg.openfga_bearer_token.clone(),
        )
    })
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
    retry::until_deadline("company seed", deadline, || {
        bootstrap::seed_company(chats.as_ref(), perms.as_ref())
    })
    .await
    .context("seeding company singleton")?;
    // Health registry is built before the dispatch chain so the chain's primary
    // hop shares the workers-gRPC breaker with the prober and `/readyz`.
    let health = Arc::new(HealthRegistry::new(&BackendId::ALL));
    let workers_grpc_breaker = health
        .breaker(BackendId::WorkersGrpc)
        .expect("workers grpc breaker registered");

    // Job dispatch chain: workers gRPC -> direct apalis push -> durable spool.
    // Both first hops land in the same apalis queue; the spool is replayed by
    // the workers' job-spool drainer.
    let jobs = ApalisNotificationQueue::new(
        retry::until_deadline("redis (apalis jobs)", deadline, || {
            jobs::notification_storage(&cfg.redis_url)
        })
        .await
        .context("connecting apalis redis (jobs)")?,
    );
    let audit_jobs = ApalisAuditQueue::new(
        retry::until_deadline("redis (apalis audit)", deadline, || {
            jobs::audit_storage(&cfg.redis_url)
        })
        .await
        .context("connecting apalis redis (audit jobs)")?,
    );
    let job_spool: Arc<dyn Spool> = Arc::new(
        retry::until_deadline("redis (job spool)", deadline, || {
            RedisSpool::new(&cfg.redis_url, "jobs")
        })
        .await
        .context("connecting redis (job spool)")?,
    );
    let grpc_dispatch: Arc<dyn JobQueue> = Arc::new(
        GrpcJobQueue::new(&cfg.workers_grpc_url, &cfg.internal_grpc_token)
            .context("building grpc job queue")?,
    );
    let notification_dispatch: Arc<dyn JobQueue> = Arc::new(DispatchQueue::new(
        grpc_dispatch.clone(),
        Arc::new(jobs),
        job_spool.clone(),
        workers_grpc_breaker.clone(),
        telemetry::current_traceparent,
    ));
    let audit_dispatch: Arc<dyn JobQueue> = Arc::new(DispatchQueue::new(
        grpc_dispatch,
        Arc::new(audit_jobs),
        job_spool,
        workers_grpc_breaker,
        telemetry::current_traceparent,
    ));
    let events = Arc::new(EventBus::new(
        publisher.clone(),
        notification_dispatch,
        audit_dispatch,
    ));
    let audit_service = Arc::new(AuditService::new(audit, perms.clone()));
    let storage_port: Arc<dyn FileStorage> = storage.clone();

    // Reporting: one Pg repo serves both the aggregate reads and the archive
    // writes; the renderer is stateless.
    let report_repo = Arc::new(PgReportingRepo::new(pool.clone()));
    let report_stats: Arc<dyn ReportStatsRepository> = report_repo.clone();
    let report_archive: Arc<dyn ReportArchiveRepository> = report_repo;
    let report_renderer: Arc<dyn ReportRenderer> = Arc::new(PrintPdfReportRenderer::new());

    // Cookie-session tokens and the real-time pub/sub handle (chat WebSocket).
    let token = Arc::new(TokenService::new(
        &cfg.jwt_secret,
        cfg.session_ttl_secs,
        cfg.cookie_secure,
    ));
    let realtime = Realtime::new(publisher, subscriber);

    // Per-backend probes (Postgres, Scylla, Redis, OpenFGA, workers gRPC). The
    // prober drives the breakers and feeds `/readyz`; it is supervised so a
    // panic restarts it. The registry itself is built earlier, next to the
    // dispatch chain that shares its workers-gRPC breaker.
    let health_checks: Vec<Arc<dyn HealthCheck>> = vec![
        Arc::new(PgHealthCheck::new(pool.clone())),
        scylla_health,
        Arc::new(
            retry::until_deadline("redis (health)", deadline, || {
                RedisHealthCheck::new(&cfg.redis_url)
            })
            .await
            .context("connecting redis (health)")?,
        ),
        Arc::new(
            OpenFgaHealthCheck::new(&cfg.openfga_api_url, cfg.openfga_bearer_token.clone())
                .context("building openfga health check")?,
        ),
        Arc::new(
            WorkersGrpcHealthCheck::new(&cfg.workers_grpc_url)
                .context("building workers grpc health check")?,
        ),
    ];
    {
        let registry = health.clone();
        let checks = health_checks;
        let interval = cfg.health_probe_interval;
        resilience::supervise("health-prober", move || {
            registry.clone().run_prober(checks.clone(), interval)
        });
    }

    // Chat service + its write-behind ingest buffer. The drain loop is spawned
    // here; `IngestShutdown` lets the serving loop flush its tail before exit. A
    // batch that can't reach Scylla is spilled to this Redis spool (durable) and
    // replayed by the workers' drainer, instead of being dropped after the
    // optimistic ack.
    let chat = Arc::new(ChatService::new(
        chats.clone(),
        users.clone(),
        chat_attachments,
        storage_port.clone(),
        perms.clone(),
        events.clone(),
    ));
    let chat_spool: Arc<dyn Spool> = Arc::new(
        retry::until_deadline("redis (chat spool)", deadline, || {
            RedisSpool::new(&cfg.redis_url, "chat")
        })
        .await
        .context("connecting redis (chat spool)")?,
    );
    let (chat_ingest, chat_ingest_rx) = ChatIngest::new(
        chat.clone(),
        chats.clone(),
        events.clone(),
        Some(chat_spool),
        ChatIngestConfig::default(),
    );
    // The drain loop owns a single-use receiver and a shutdown handshake that
    // flushes its tail on exit, so it keeps its own spawn rather than the restart
    // supervisor; a persist failure now spills instead of crashing the loop.
    let (ingest_shutdown_tx, ingest_shutdown_rx) = oneshot::channel();
    let ingest_drain = tokio::spawn(chat_ingest.clone().run(chat_ingest_rx, ingest_shutdown_rx));
    let ingest_shutdown = IngestShutdown {
        trigger: ingest_shutdown_tx,
        drain: ingest_drain,
    };

    // Attendance policy: load the singleton into a cached provider at boot so reads
    // are lock-free. Other attendance services share this `policy_provider`.
    let policy_repo: Arc<dyn PolicyRepository> = Arc::new(PgPolicyRepo::new(pool.clone()));
    let policy_provider = Arc::new(PolicyProvider::new(
        retry::until_deadline("postgres (policy)", deadline, || policy_repo.load())
            .await
            .context("loading attendance policy")?,
    ));

    // Request service is hoisted so the daily-report service can bump request
    // progress when a report's RequestWork entries carry a completion hint.
    let request_service = Arc::new(RequestService::new(
        requests.clone(),
        projects.clone(),
        groups.clone(),
        storage_port.clone(),
        perms.clone(),
        events.clone(),
    ));
    let daily_report_repo: Arc<dyn DailyReportRepository> =
        Arc::new(PgDailyReportRepo::new(pool.clone()));

    // Leave subsystem. No cycle: LeaveBalanceService depends on the DayOff
    // repository trait, while DayOffService depends on the LeaveBalance service.
    let holiday_repo: Arc<dyn HolidayRepository> = Arc::new(PgHolidayRepo::new(pool.clone()));
    let leave_repo: Arc<dyn LeaveBalanceRepository> =
        Arc::new(PgLeaveBalanceRepo::new(pool.clone()));
    let day_off_repo: Arc<dyn DayOffRepository> = Arc::new(PgDayOffRepo::new(pool.clone()));
    let holiday_service = Arc::new(HolidayService::new(holiday_repo.clone(), perms.clone()));
    let leave_service = Arc::new(LeaveBalanceService::new(
        leave_repo,
        holiday_repo.clone(),
        day_off_repo.clone(),
        policy_provider.clone(),
        perms.clone(),
        events.clone(),
    ));
    let day_off_service = Arc::new(DayOffService::new(
        day_off_repo,
        holiday_repo,
        leave_service.clone(),
        perms.clone(),
        events.clone(),
    ));

    // Overtime: leader + HR approval, capped monthly by the cached policy.
    let overtime_repo: Arc<dyn OvertimeRepository> = Arc::new(PgOvertimeRepo::new(pool.clone()));
    let overtime_service = Arc::new(OvertimeService::new(
        overtime_repo,
        policy_provider.clone(),
        perms.clone(),
        events.clone(),
    ));

    // Flexible hours: leader approval, per-day shape + monthly cap from policy.
    let flex_repo: Arc<dyn FlexHoursRepository> = Arc::new(PgFlexRepo::new(pool.clone()));
    let flex_service = Arc::new(FlexHoursService::new(
        flex_repo,
        policy_provider.clone(),
        perms.clone(),
        events.clone(),
    ));

    // Hoisted: shared by the HTTP report routes and the internal Query plane.
    let report_service = Arc::new(ReportService::new(
        report_stats,
        report_archive,
        report_renderer,
        storage_port.clone(),
        users.clone(),
        leave_service.clone(),
        flex_service.clone(),
        perms.clone(),
    ));

    // External API: admin-issued keys + the scope-gated read-only queries.
    let service_account_repo: Arc<dyn ServiceAccountRepository> =
        Arc::new(PgServiceAccountRepo::new(pool.clone()));
    let service_account_service = Arc::new(ServiceAccountService::new(
        service_account_repo,
        perms.clone(),
    ));
    let read_plane = Arc::new(ReadPlaneService::new(
        projects.clone(),
        requests.clone(),
        report_service.clone(),
    ));
    let ext_read_service = Arc::new(ExtReadService::new(read_plane.clone(), perms.clone()));

    let state = AppState {
        user: Arc::new(UserService::new(
            users.clone(),
            groups.clone(),
            requests.clone(),
            chats.clone(),
            perms.clone(),
            events.clone(),
            revocation.clone(),
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
        request: request_service.clone(),
        ticket: Arc::new(TicketService::new(
            tickets.clone(),
            perms.clone(),
            events.clone(),
        )),
        chat,
        chat_ingest,
        comment: Arc::new(CommentService::new(
            comments,
            requests.clone(),
            tickets.clone(),
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
        report: report_service.clone(),
        service_accounts: service_account_service,
        ext_read: ext_read_service,
        policy: Arc::new(PolicyService::new(
            policy_repo.clone(),
            policy_provider.clone(),
            perms.clone(),
            events.clone(),
        )),
        daily_report: Arc::new(DailyReportService::new(
            daily_report_repo,
            groups.clone(),
            request_service.clone(),
            perms.clone(),
            events.clone(),
        )),
        holiday: holiday_service,
        leave: leave_service,
        day_off: day_off_service,
        overtime: overtime_service,
        flex: flex_service,
        perms: perms.clone(),
        token,
        revocation,
        realtime,
        summary_cache: Arc::new(resolve::SummaryCache::new(Duration::from_secs(30))),
        audit: audit_service,
        presence,
        rate_limiter,
        rate_limits: RateLimits {
            auth: cfg.auth_rate_limit,
            auth_ip: cfg.auth_ip_rate_limit,
            api: cfg.api_rate_limit,
            chat: cfg.chat_rate_limit,
            ext: cfg.ext_rate_limit,
            ext_ip: cfg.ext_ip_rate_limit,
        },
        ip_allowlist: IpAllowlist {
            enabled: cfg.ip_allowlist_enabled,
            nets: cfg.ip_allowlist.iter().copied().collect(),
            trusted_proxies: cfg.trusted_proxies.iter().copied().collect(),
        },
        storage,
        signed_url,
        health,
    };

    let grpc = GrpcPlane::new(
        cfg.grpc_addr,
        cfg.internal_grpc_token.clone(),
        QueryService::new(read_plane),
    );
    let app = router(state).layer(cors_layer(&cfg.cors_allowed_origins));
    Ok((app, ingest_shutdown, grpc))
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
        .merge(users::router())
        .merge(groups::router())
        .merge(projects::router())
        .merge(requests::router())
        .merge(tickets::router())
        .merge(chat::router())
        .merge(chat_ws::router())
        .merge(announcements::router())
        .merge(notifications::router())
        .merge(audit::router())
        .merge(reports::router())
        .merge(policy::router())
        .merge(daily_reports::router())
        .merge(holidays::router())
        .merge(leave_balance::router())
        .merge(day_off::router())
        .merge(overtime::router())
        .merge(flex_hours::router())
        .merge(service_accounts::router())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit::per_user,
        ))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    // Public API: login / logout carry no session yet; rate-limited per client IP.
    let public = routes::auth::public_router().route_layer(middleware::from_fn_with_state(
        state.clone(),
        rate_limit::per_ip,
    ));

    // Files need session + signed `?exp&sig` (bound to the user); skip the per-user limiter -
    // the signature already scopes each fetch, so image-heavy pages don't burn the API budget.
    let files = files::router().route_layer(middleware::from_fn_with_state(
        state.clone(),
        auth::require_auth,
    ));
    // Timeout + load-shed guard the API tree only: /healthz (liveness) must keep
    // answering under overload, and /readyz reads breaker state, not backends.
    let api = public
        .merge(protected)
        .merge(files)
        // Backstop deadline: each backend call is individually bounded; this caps
        // the whole request. 30s leaves room for 25 MiB uploads on slow links.
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        // One stack so the fallible load-shed error is handled before axum sees
        // it: at capacity new requests get an immediate 503 instead of queueing.
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(on_overload))
                .layer(LoadShedLayer::new())
                .layer(GlobalConcurrencyLimitLayer::new(MAX_IN_FLIGHT)),
        );

    // External read-only surface for scripts: service-account bearer keys, no
    // cookies, per-key limit inside the auth middleware. Own timeout; kept out
    // of the SPA tree's load-shed so a script burst can't starve the app (the
    // per-key limiter bounds it instead).
    let ext_api = ext::router()
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            service_account::require_service_account,
        ))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ));

    Router::new()
        .route("/healthz", routing::get(healthz))
        .route("/readyz", routing::get(readyz))
        .nest("/api/v1", api)
        .nest("/api/ext/v1", ext_api)
        // Applied outermost-to-innermost: trace, request-id, security headers, body
        // limit, catch-panic, then the router (its protected sub-tree adds auth +
        // limit); CORS is added by the caller.
        //
        // Catch-panic is innermost so it wraps the handlers directly: a handler
        // panic becomes a logged 500 instead of a dropped connection, and the
        // outer layers still see a normal response.
        .layer(CatchPanicLayer::custom(on_panic))
        .layer(CompressionLayer::new())
        // Global 1 MiB JSON cap; upload routes override with their own DefaultBodyLimit.
        .layer(DefaultBodyLimit::max(1024 * 1024))
        // Baseline security headers (no HSTS - internal plain HTTP); `if_not_present` lets a route override.
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
        ))
        // Network gate: rejects out-of-allowlist peers before auth/handlers, but
        // inside trace + request-id so a 403 is still traced and correlated.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            ip_allowlist::enforce,
        ))
        .layer(middleware::from_fn(request_id::propagate))
        .layer(trace::layer())
        .with_state(state)
}

/// Builds the credentialed CORS layer from the origins already validated by
/// `config::from_env` (a malformed origin fails startup there). Credentialed
/// CORS forbids a wildcard origin, so the allow-list is always explicit.
pub fn cors_layer(origins: &[HeaderValue]) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins.iter().cloned()))
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

/// Ceiling on concurrently-served requests; excess is shed with 503. WS
/// connections release their slot at upgrade, so this bounds request work, not
/// connected users.
const MAX_IN_FLIGHT: usize = 512;

/// Responder for the load-shed layer: the only error it can surface is
/// `Overloaded`, mapped to a structured 503.
async fn on_overload(_: BoxError) -> Response {
    let body = ApiError {
        code: ErrorCode::Internal,
        message: "server overloaded, retry shortly".to_owned(),
    };
    (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response()
}

/// Process liveness: returns `200 ok` as long as the process is up. Deliberately
/// dependency-free so it never flaps on a backend outage.
async fn healthz() -> &'static str {
    "ok"
}

/// Readiness: reports each backend's health from the circuit breakers and returns
/// `503` when any backend is `Down`, so a load balancer / orchestrator drains
/// this instance while it can't serve, and `200` while all are `Up`/`Degraded`.
async fn readyz(State(state): State<AppState>) -> Response {
    let snapshot = state.health.status();
    let backends: Vec<BackendHealth> = snapshot
        .iter()
        .map(|(id, status)| BackendHealth {
            backend: id.as_str().to_owned(),
            status: map_status(*status),
        })
        .collect();
    let overall = overall_status(&snapshot);
    let code = if overall == BackendStatus::Down {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };
    (
        code,
        Json(ReadinessResponse {
            status: overall,
            backends,
        }),
    )
        .into_response()
}

/// Catch-panic responder: logs the panic payload and returns the same
/// `{ code, message }` 500 body the rest of the API emits, so a handler panic is
/// a structured error response rather than a dropped connection.
#[allow(clippy::needless_pass_by_value)]
fn on_panic(err: Box<dyn Any + Send + 'static>) -> Response {
    let message = err
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| err.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string panic payload>");
    tracing::error!(target: "panic", panic = message, "request handler panicked; returning 500");
    let body = ApiError {
        code: ErrorCode::Internal,
        message: "internal server error".to_owned(),
    };
    (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
}

fn map_status(status: HealthStatus) -> BackendStatus {
    match status {
        HealthStatus::Up => BackendStatus::Up,
        HealthStatus::Degraded => BackendStatus::Degraded,
        HealthStatus::Down => BackendStatus::Down,
    }
}

/// Worst status across all backends: any gating `Down` -> `Down`, else
/// anything short of `Up` -> `Degraded`, else `Up`. Non-gating backends
/// (workers gRPC) surface as at most `Degraded`: job dispatch survives them
/// via the fallback hops, so they must not drain the instance.
fn overall_status(snapshot: &[(BackendId, HealthStatus)]) -> BackendStatus {
    if snapshot
        .iter()
        .any(|(id, s)| *s == HealthStatus::Down && id.gates_readiness())
    {
        BackendStatus::Down
    } else if snapshot.iter().any(|(_, s)| *s != HealthStatus::Up) {
        BackendStatus::Degraded
    } else {
        BackendStatus::Up
    }
}
