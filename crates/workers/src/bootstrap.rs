use std::{sync::Arc, time::Instant};

use anyhow::Context;
use apalis_redis::RedisStorage;
use tokio::fs;

use application::{
    AuditProjector, EmailNotifier, FlexHoursService, LeaveBalanceService, MaintenanceService,
    NotificationFanout, PolicyProvider, RepairService, ReportService,
    events::EventBus,
    permissions::Permissions,
    resilience::{CircuitBreaker, Drainer, DrainerConfig, HealthRegistry, retry},
};
use domain::{
    health::BackendId,
    ports::{
        file_storage::FileStorage, health::HealthCheck, job_queue::JobQueue, mailer::Mailer,
        report_renderer::ReportRenderer, spool::Spool, token_revocation::TokenRevocation,
    },
    repository::{
        AuditRepository, ChatAttachmentRepository, ChatRepository, DayOffRepository,
        FlexHoursRepository, GroupRepository, HolidayRepository, LeaveBalanceRepository,
        NotificationRepository, OutboxRepository, PolicyRepository, ProjectRepository,
        ReportArchiveRepository, ReportStatsRepository, RequestRepository, TicketRepository,
        UserRepository,
    },
};
use infrastructure::{
    health::{PgHealthCheck, RedisHealthCheck, ScyllaHealthCheck},
    jobs::{
        self, ApalisEmailQueue, ApalisNotificationQueue, EmailEnvelope, NotificationEnvelope,
        RepairEnvelope,
    },
    local_storage::LocalStorage,
    mailer::{LogMailer, SmtpMailer},
    openfga::{self, OpenFgaAuthzClient},
    postgres::{
        self, PgAuditRepo, PgChatAttachmentRepo, PgDayOffRepo, PgFlexRepo, PgGroupRepo,
        PgHolidayRepo, PgLeaveBalanceRepo, PgNotificationRepo, PgOutboxRepo, PgPolicyRepo,
        PgProjectRepo, PgReportingRepo, PgRequestRepo, PgTicketRepo, PgUserRepo,
    },
    redis::{RedisEventPublisher, RedisSpool, RedisTokenRevocation},
    report::PrintPdfReportRenderer,
    scylla::{self, ScyllaChatRepo},
    signed_url::SignedUrl,
};

use crate::config::Config;

/// Everything the job handlers need, assembled once at startup.
pub struct WorkerContext {
    pub fanout: Arc<NotificationFanout>,
    pub storage: RedisStorage<NotificationEnvelope>,
    /// Outbox-driven audit projector, drained by a supervised poll loop.
    pub audit_projector: Arc<AuditProjector>,
    /// Worker-side reconciles for post-commit obligations that failed inline.
    pub repair_service: Arc<RepairService>,
    pub repair_storage: RedisStorage<RepairEnvelope>,
    pub maintenance: Arc<MaintenanceService>,
    pub mailer: Arc<dyn Mailer>,
    pub email_storage: RedisStorage<EmailEnvelope>,
    pub report: Arc<ReportService>,
    /// Leave-balance service driving the scheduled expiry sweep.
    pub leave: Arc<LeaveBalanceService>,
    /// Flex-hours service driving the month-end reconciliation sweep.
    pub flex: Arc<FlexHoursService>,
    pub email_queue: Arc<dyn JobQueue>,
    /// Per-backend breakers + the prober that drives them.
    pub health_registry: Arc<HealthRegistry>,
    pub health_checks: Vec<Arc<dyn HealthCheck>>,
    /// Postgres breaker, shared into the PG-writing job handlers as a fail-fast gate.
    pub pg_breaker: Arc<CircuitBreaker>,
    /// Replays chat batches that couldn't reach Scylla during an outage.
    pub chat_drainer: Drainer,
    /// Job dispatches the server spooled while both enqueue hops were down.
    pub job_spool: Arc<dyn Spool>,
}

/// Builds the infrastructure adapters and opens the apalis job storage the worker
/// consumes. Mirrors the server composition root, minus the HTTP/authz slice.
#[allow(clippy::too_many_lines)]
pub async fn build(cfg: &Config) -> anyhow::Result<WorkerContext> {
    // Every connect below retries with backoff against this shared deadline, so
    // a binary started before its infra waits instead of failing.
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

    let notifications: Arc<dyn NotificationRepository> =
        Arc::new(PgNotificationRepo::new(pool.clone()));
    let groups: Arc<dyn GroupRepository> = Arc::new(PgGroupRepo::new(pool.clone()));
    let users: Arc<dyn UserRepository> = Arc::new(PgUserRepo::new(pool.clone()));
    let requests: Arc<dyn RequestRepository> = Arc::new(PgRequestRepo::new(pool.clone()));
    // Clone the session for the health probe before the repo takes ownership.
    let scylla_health: Arc<dyn HealthCheck> = Arc::new(ScyllaHealthCheck::new(session.clone()));
    let chats: Arc<dyn ChatRepository> = Arc::new(
        retry::until_deadline("scylla", deadline, || ScyllaChatRepo::new(session.clone()))
            .await
            .context("preparing scylla statements")?,
    );
    let tickets: Arc<dyn TicketRepository> = Arc::new(PgTicketRepo::new(pool.clone()));
    let projects: Arc<dyn ProjectRepository> = Arc::new(PgProjectRepo::new(pool.clone()));
    let signer = Arc::new(SignedUrl::new(cfg.storage_signing_secret.as_bytes()));
    let file_storage: Arc<dyn FileStorage> = Arc::new(LocalStorage::new(
        cfg.storage_root.clone(),
        &cfg.storage_public_base,
        signer,
    ));

    let storage = retry::until_deadline("redis (apalis jobs)", deadline, || {
        jobs::notification_storage(&cfg.redis_url)
    })
    .await
    .context("connecting apalis redis (jobs)")?;
    let email_store = retry::until_deadline("redis (apalis email)", deadline, || {
        jobs::email_storage(&cfg.redis_url)
    })
    .await
    .context("connecting apalis redis (email jobs)")?;
    let repair_storage = retry::until_deadline("redis (apalis repair)", deadline, || {
        jobs::repair_storage(&cfg.redis_url)
    })
    .await
    .context("connecting apalis redis (repair jobs)")?;

    // Event bus for system events (ticket auto-close): broadcast publisher plus the two durable queues this process consumes.
    let publisher = Arc::new(
        retry::until_deadline("redis (events)", deadline, || {
            RedisEventPublisher::new(&cfg.redis_url)
        })
        .await
        .context("connecting redis (events)")?,
    );
    let events = Arc::new(EventBus::new(
        publisher,
        Arc::new(ApalisNotificationQueue::new(storage.clone())),
    ));

    // Leave-expiry sweep service; Permissions only satisfies the constructor (the sweep uses no authz), PolicyProvider supplies the expiry window.
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
    let perms = Arc::new(Permissions::new(
        users.clone(),
        groups.clone(),
        Arc::new(authz),
    ));
    let policy_repo: Arc<dyn PolicyRepository> = Arc::new(PgPolicyRepo::new(pool.clone()));
    let policy_provider = Arc::new(PolicyProvider::new(
        retry::until_deadline("postgres (policy)", deadline, || policy_repo.load())
            .await
            .context("loading attendance policy")?,
    ));
    let holiday_repo: Arc<dyn HolidayRepository> = Arc::new(PgHolidayRepo::new(pool.clone()));
    let leave_repo: Arc<dyn LeaveBalanceRepository> =
        Arc::new(PgLeaveBalanceRepo::new(pool.clone()));
    let day_off_repo: Arc<dyn DayOffRepository> = Arc::new(PgDayOffRepo::new(pool.clone()));
    let leave = Arc::new(LeaveBalanceService::new(
        leave_repo,
        holiday_repo,
        day_off_repo.clone(),
        policy_provider.clone(),
        perms.clone(),
        events.clone(),
    ));

    // Flex reconciliation sweep service; the constructor wants Permissions though the sweep uses no authz.
    let flex_repo: Arc<dyn FlexHoursRepository> = Arc::new(PgFlexRepo::new(pool.clone()));
    let flex = Arc::new(FlexHoursService::new(
        flex_repo,
        policy_provider,
        perms.clone(),
        events.clone(),
    ));

    // Reporting: one Pg repo backs the aggregate reads and the archive; the renderer is stateless.
    let report_repo = Arc::new(PgReportingRepo::new(pool.clone()));
    let report_stats: Arc<dyn ReportStatsRepository> = report_repo.clone();
    let report_archive: Arc<dyn ReportArchiveRepository> = report_repo;
    let report_renderer: Arc<dyn ReportRenderer> = Arc::new(PrintPdfReportRenderer::new());
    let report = Arc::new(ReportService::new(
        report_stats,
        report_archive.clone(),
        report_renderer,
        file_storage.clone(),
        users.clone(),
        leave.clone(),
        flex.clone(),
        perms.clone(),
    ));

    // Built before the fan-out moves the repo handles below.
    let chat_attachments: Arc<dyn ChatAttachmentRepository> =
        Arc::new(PgChatAttachmentRepo::new(pool.clone()));
    let maintenance = Arc::new(MaintenanceService::new(
        notifications.clone(),
        requests.clone(),
        tickets.clone(),
        chat_attachments,
        users.clone(),
        report_archive,
        file_storage,
        events,
    ));

    let audit: Arc<dyn AuditRepository> = Arc::new(PgAuditRepo::new(pool.clone()));
    let audit_outbox: Arc<dyn OutboxRepository> = Arc::new(PgOutboxRepo::new(pool.clone()));
    let audit_projector = Arc::new(AuditProjector::new(audit, audit_outbox));

    // Repair reconciles re-derive tuples/rows from the DB; revocation mirrors
    // the server's Redis-backed session-version store.
    let revocation: Arc<dyn TokenRevocation> = Arc::new(
        retry::until_deadline("redis (token revocation)", deadline, || {
            RedisTokenRevocation::new(&cfg.redis_url, cfg.session_ttl_secs * 2)
        })
        .await
        .context("connecting redis (token revocation)")?,
    );
    let repair_service = Arc::new(RepairService::new(
        users.clone(),
        groups.clone(),
        projects.clone(),
        tickets.clone(),
        day_off_repo.clone(),
        chats.clone(),
        perms.clone(),
        leave.clone(),
        revocation,
    ));

    // SMTP when configured, log-only otherwise.
    let mailer: Arc<dyn Mailer> = match &cfg.email {
        Some(smtp) => Arc::new(
            SmtpMailer::new(
                &smtp.host,
                smtp.port,
                smtp.username.as_deref(),
                smtp.password.as_deref(),
                &smtp.from,
                smtp.tls,
            )
            .context("building smtp mailer")?,
        ),
        None => Arc::new(LogMailer),
    };
    let notifier = Arc::new(EmailNotifier::new(
        users.clone(),
        Arc::new(ApalisEmailQueue::new(email_store.clone())),
        &cfg.portal_base_url,
    ));
    // Dedicated queue handle for the report scheduler's email fan-out.
    let email_queue: Arc<dyn JobQueue> = Arc::new(ApalisEmailQueue::new(email_store.clone()));

    // Health registry + per-backend probes; workers touch Postgres, Scylla, and Redis (no OpenFGA).
    let health_registry = Arc::new(HealthRegistry::new(&[
        BackendId::Postgres,
        BackendId::Scylla,
        BackendId::Redis,
    ]));
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
    ];
    let pg_breaker = health_registry
        .breaker(BackendId::Postgres)
        .expect("postgres breaker registered");
    let scylla_breaker = health_registry
        .breaker(BackendId::Scylla)
        .expect("scylla breaker registered");

    // Chat write-behind spool drainer: replays batches the server spooled while
    // Scylla was down, paced by the Scylla breaker.
    let spool: Arc<dyn Spool> = Arc::new(
        retry::until_deadline("redis (chat spool)", deadline, || {
            RedisSpool::new(&cfg.redis_url, "chat")
        })
        .await
        .context("connecting redis (chat spool)")?,
    );
    let chat_drainer = Drainer::new(
        spool,
        chats.clone(),
        scylla_breaker,
        DrainerConfig::default(),
    );

    // Job-dispatch spool: the server's last-resort hop when gRPC and the direct
    // apalis push are both unavailable; replayed by the job-spool drainer.
    let job_spool: Arc<dyn Spool> = Arc::new(
        retry::until_deadline("redis (job spool)", deadline, || {
            RedisSpool::new(&cfg.redis_url, "jobs")
        })
        .await
        .context("connecting redis (job spool)")?,
    );

    let fanout = Arc::new(
        NotificationFanout::new(
            notifications,
            groups,
            users,
            requests,
            chats,
            tickets,
            projects,
        )
        .with_email(notifier),
    );

    Ok(WorkerContext {
        fanout,
        storage,
        audit_projector,
        repair_service,
        repair_storage,
        maintenance,
        mailer,
        email_storage: email_store,
        report,
        leave,
        flex,
        email_queue,
        health_registry,
        health_checks,
        pg_breaker,
        chat_drainer,
        job_spool,
    })
}
