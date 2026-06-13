use std::sync::Arc;

use anyhow::Context;
use apalis_redis::RedisStorage;

use application::{
    AuditProjector, EmailNotifier, MaintenanceService, NotificationFanout, events::EventBus,
};
use domain::{
    ports::{file_storage::FileStorage, mailer::Mailer},
    repository::{
        AuditRepository, ChatAttachmentRepository, ChatRepository, GroupRepository,
        NotificationRepository, ProjectRepository, RequestRepository, TicketRepository,
        UserRepository,
    },
};
use infrastructure::{
    jobs::{
        ApalisAuditQueue, ApalisEmailQueue, ApalisNotificationQueue, AuditEnvelope, EmailEnvelope,
        NotificationEnvelope, audit_storage, email_storage, notification_storage,
    },
    local_storage::LocalStorage,
    mailer::{LogMailer, SmtpMailer, SmtpTls},
    postgres::{
        PgAuditRepo, PgChatAttachmentRepo, PgGroupRepo, PgNotificationRepo, PgProjectRepo,
        PgRequestRepo, PgTicketRepo, PgUserRepo, build_pool,
    },
    redis::RedisEventPublisher,
    scylla::{ScyllaChatRepo, build_session},
    signed_url::SignedUrl,
};

use crate::config::Config;

/// Everything the job handlers need, assembled once at startup.
pub struct WorkerContext {
    pub fanout: Arc<NotificationFanout>,
    pub storage: RedisStorage<NotificationEnvelope>,
    pub audit_projector: Arc<AuditProjector>,
    pub audit_storage: RedisStorage<AuditEnvelope>,
    pub maintenance: Arc<MaintenanceService>,
    pub mailer: Arc<dyn Mailer>,
    pub email_storage: RedisStorage<EmailEnvelope>,
}

/// Builds the infrastructure adapters, wires the notification fan-out service,
/// and opens the apalis job storage the worker consumes. Mirrors the server
/// composition root, minus the HTTP/authz slice the fan-out does not need
/// (fan-out runs as the system, so no `Permissions`/`OpenFGA`).
pub async fn build(cfg: &Config) -> anyhow::Result<WorkerContext> {
    let pool = build_pool(&cfg.database_url, cfg.pg_max_connections)
        .await
        .context("building postgres pool")?;
    let session = build_session(&cfg.scylla_hosts, &cfg.scylla_keyspace)
        .await
        .context("building scylla session")?;

    let notifications: Arc<dyn NotificationRepository> =
        Arc::new(PgNotificationRepo::new(pool.clone()));
    let groups: Arc<dyn GroupRepository> = Arc::new(PgGroupRepo::new(pool.clone()));
    let users: Arc<dyn UserRepository> = Arc::new(PgUserRepo::new(pool.clone()));
    let requests: Arc<dyn RequestRepository> = Arc::new(PgRequestRepo::new(pool.clone()));
    let chats: Arc<dyn ChatRepository> = Arc::new(
        ScyllaChatRepo::new(session)
            .await
            .context("preparing scylla statements")?,
    );
    let tickets: Arc<dyn TicketRepository> = Arc::new(PgTicketRepo::new(pool.clone()));
    let projects: Arc<dyn ProjectRepository> = Arc::new(PgProjectRepo::new(pool.clone()));
    // The orphan-sweep job only lists and deletes; it never presigns, so this
    // signer is never exercised (hence the empty key).
    let signer = Arc::new(SignedUrl::new(b""));
    let file_storage: Arc<dyn FileStorage> = Arc::new(LocalStorage::new(
        cfg.storage_root.clone(),
        &cfg.storage_public_base,
        signer,
    ));

    let storage = notification_storage(&cfg.redis_url)
        .await
        .context("connecting apalis redis (jobs)")?;
    let audit_storage = audit_storage(&cfg.redis_url)
        .await
        .context("connecting apalis redis (audit jobs)")?;
    let email_store = email_storage(&cfg.redis_url)
        .await
        .context("connecting apalis redis (email jobs)")?;

    // Event bus for system events (ticket auto-close); same wiring as the server -
    // broadcast publisher + the two durable queues this process consumes.
    let publisher = Arc::new(
        RedisEventPublisher::new(&cfg.redis_url)
            .await
            .context("connecting redis (events)")?,
    );
    let events = Arc::new(EventBus::new(
        publisher,
        Arc::new(ApalisNotificationQueue::new(storage.clone())),
        Arc::new(ApalisAuditQueue::new(audit_storage.clone())),
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
        file_storage,
        events,
    ));

    let audit: Arc<dyn AuditRepository> = Arc::new(PgAuditRepo::new(pool.clone()));
    let audit_projector = Arc::new(AuditProjector::new(audit));

    // SMTP when enabled, log-only otherwise (config validated host/from).
    let mailer: Arc<dyn Mailer> = if cfg.email_enabled {
        let tls = match cfg.smtp_tls.as_str() {
            "none" => SmtpTls::None,
            _ => SmtpTls::StartTls,
        };
        Arc::new(
            SmtpMailer::new(
                cfg.smtp_host.as_deref().unwrap_or_default(),
                cfg.smtp_port,
                cfg.smtp_username.as_deref(),
                cfg.smtp_password.as_deref(),
                cfg.smtp_from.as_deref().unwrap_or_default(),
                tls,
            )
            .context("building smtp mailer")?,
        )
    } else {
        Arc::new(LogMailer)
    };
    let notifier = Arc::new(EmailNotifier::new(
        users.clone(),
        Arc::new(ApalisEmailQueue::new(email_store.clone())),
        cfg.portal_base_url.clone(),
    ));

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
        audit_storage,
        maintenance,
        mailer,
        email_storage: email_store,
    })
}
