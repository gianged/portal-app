-- =============================================================================
-- init.sql
-- Schema for: Internal Portal (single-company, 100-1000 users)
-- Apply:   psql -h localhost -U portal -d portal -f infra/postgres/init.sql
--
-- Single source of truth for the relational schema. Change it here directly,
-- then reinitialize the dev database (Postgres only runs this on an empty
-- volume): `docker compose --env-file .env -f infra/docker-compose.infra.yml
-- down -v` followed by `cargo make bootstrap`, then `cargo make sqlx-prepare`.
--
-- Postgres holds the relational system-of-record: identity, org structure,
-- projects, requests, tickets, notifications, audit. The following live
-- elsewhere by design:
--   - Chat / channels / messages / announcements -> Cassandra
--   - Authorization tuples (permissions, ReBAC graph) -> OpenFGA
--   - Sessions / presence / pub-sub / rate-limit  -> Redis
--   - File payloads (avatars, attachments)        -> MinIO (only the
--                                                    storage_key is here)
-- =============================================================================


-- -----------------------------------------------------------------------------
-- 1. Extensions
-- -----------------------------------------------------------------------------
CREATE EXTENSION IF NOT EXISTS pgcrypto;     -- gen_random_uuid()
-- CREATE EXTENSION IF NOT EXISTS citext;    -- enable if email comparison
                                             -- should be case-insensitive
                                             -- at the type level
-- CREATE EXTENSION IF NOT EXISTS pg_trgm;   -- enable for trigram search on
                                             -- titles / descriptions


-- -----------------------------------------------------------------------------
-- 2. Schemas (alphabetical)
-- -----------------------------------------------------------------------------
CREATE SCHEMA IF NOT EXISTS attendance;
CREATE SCHEMA IF NOT EXISTS auth;
CREATE SCHEMA IF NOT EXISTS audit;
CREATE SCHEMA IF NOT EXISTS chat;
CREATE SCHEMA IF NOT EXISTS notification;
CREATE SCHEMA IF NOT EXISTS org;
CREATE SCHEMA IF NOT EXISTS project;
CREATE SCHEMA IF NOT EXISTS reporting;
CREATE SCHEMA IF NOT EXISTS ticket;


-- -----------------------------------------------------------------------------
-- 3. Enums (per-schema, alphabetical)
-- -----------------------------------------------------------------------------

-- attendance
CREATE TYPE attendance.balance_expiry_policy AS ENUM (
    'warn',
    'record_work_pct'
);

CREATE TYPE attendance.daily_report_status AS ENUM (
    'draft',
    'submitted',
    'approved',
    'returned'
);

CREATE TYPE attendance.daily_report_entry_kind AS ENUM (
    'request_work',
    'learning',
    'other'
);

CREATE TYPE attendance.leave_txn_kind AS ENUM (
    'grant',
    'consume',
    'refund',
    'adjust',
    'expire'
);

CREATE TYPE attendance.dayoff_kind AS ENUM (
    'annual_leave',
    'sick_leave',
    'unpaid_leave',
    'remote',
    'other'
);

CREATE TYPE attendance.dayoff_status AS ENUM (
    'pending',
    'leader_approved',
    'approved',
    'rejected',
    'cancelled'
);

CREATE TYPE attendance.overtime_status AS ENUM (
    'pending',
    'leader_approved',
    'approved',
    'rejected',
    'cancelled'
);

CREATE TYPE attendance.flex_status AS ENUM (
    'pending',
    'approved',
    'rejected',
    'cancelled'
);

-- auth
CREATE TYPE auth.service_account_status AS ENUM (
    'active',
    'revoked'
);

CREATE TYPE auth.system_role AS ENUM (
    'director',
    'hr'
);

CREATE TYPE auth.user_status AS ENUM (
    'pending',
    'active',
    'deactivated'
);

-- audit
CREATE TYPE audit.audit_action AS ENUM (
    'create',
    'update',
    'delete',
    'status_change',
    'assign',
    'transfer'
);

-- notification
CREATE TYPE notification.notification_kind AS ENUM (
    'announcement',
    'mention',
    'ticket_urgent',
    'request_assigned',
    'request_status_change',
    'project_invite',
    'ticket_assigned',
    'ticket_status_change',
    'project_invite_response',
    'ticket_raised',
    'system',
    'request_comment',
    'ticket_comment'
);

-- org
CREATE TYPE org.group_kind AS ENUM (
    'standard',
    'it'
);

CREATE TYPE org.group_role AS ENUM (
    'leader',
    'sub_leader',
    'member'
);

-- project
CREATE TYPE project.invite_status AS ENUM (
    'pending',
    'accepted',
    'declined',
    'revoked'
);

CREATE TYPE project.project_status AS ENUM (
    'planning',
    'active',
    'on_hold',
    'completed',
    'cancelled'
);

CREATE TYPE project.request_priority AS ENUM (
    'low',
    'normal',
    'high',
    'urgent'
);

CREATE TYPE project.request_status AS ENUM (
    'draft',
    'submitted',
    'assigned',
    'in_progress',
    'review',
    'completed',
    'cancelled'
);

-- reporting
CREATE TYPE reporting.report_kind AS ENUM (
    'monthly',
    'yearly'
);

CREATE TYPE reporting.report_scope AS ENUM (
    'company',
    'group',
    'staff'
);

-- ticket
CREATE TYPE ticket.ticket_category AS ENUM (
    'hardware',
    'software',
    'access',
    'other'
);

CREATE TYPE ticket.ticket_priority AS ENUM (
    'low',
    'normal',
    'high',
    'urgent'
);

CREATE TYPE ticket.ticket_status AS ENUM (
    'open',
    'triaged',
    'assigned',
    'in_progress',
    'resolved',
    'closed',
    'reopened'
);


-- -----------------------------------------------------------------------------
-- 4. Functions
-- -----------------------------------------------------------------------------

-- Shared updated_at maintainer. Wired up per-table in section 8.
CREATE OR REPLACE FUNCTION public.fn_set_updated_at()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;

-- Invariant 5: audit rows are append-only. Wired to audit.audit_log in
-- section 8; a compromised connection or code bug cannot rewrite history.
CREATE OR REPLACE FUNCTION audit.fn_forbid_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'audit.audit_log is append-only (% blocked)', TG_OP
        USING ERRCODE = 'insufficient_privilege';
END;
$$;

-- Cross-table invariant: a project's owner_group cannot also be one of its
-- collaborator groups (invariant 7 in domain-logic.txt). A plain CHECK
-- constraint can't reference another table, so we use a trigger.
CREATE OR REPLACE FUNCTION project.fn_no_self_collab()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
DECLARE
    v_owner_group_id UUID;
BEGIN
    SELECT owner_group_id INTO v_owner_group_id
    FROM project.projects
    WHERE id = NEW.project_id;

    IF NEW.group_id = v_owner_group_id THEN
        RAISE EXCEPTION
            'project owner group cannot also be a collaborator (project_id=%, group_id=%)',
            NEW.project_id, NEW.group_id
            USING ERRCODE = 'check_violation';
    END IF;

    RETURN NEW;
END;
$$;


-- -----------------------------------------------------------------------------
-- 5. Tables
--    Grouped by schema (alphabetical), with FK constraints deferred to
--    section 6 so file order is independent of dependency order.
-- -----------------------------------------------------------------------------

-- auth.users -----------------------------------------------------------------
CREATE TABLE auth.users (
    id                  UUID        NOT NULL DEFAULT gen_random_uuid(),
    email               TEXT        NOT NULL,
    password_hash       TEXT        NOT NULL,
    full_name           TEXT        NOT NULL,
    avatar_storage_key  TEXT,
    phone               TEXT,
    timezone            TEXT        NOT NULL DEFAULT 'UTC',
    status              auth.user_status NOT NULL DEFAULT 'pending',
    system_role         auth.system_role,
    email_notifications BOOLEAN     NOT NULL DEFAULT TRUE,
    first_logged_in_at  TIMESTAMPTZ,
    deactivated_at      TIMESTAMPTZ,
    -- optimistic-lock counter; guarded upserts bump it on every update
    version             BIGINT      NOT NULL DEFAULT 0,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_users PRIMARY KEY (id),
    CONSTRAINT uq_users_email UNIQUE (email),
    CONSTRAINT chk_users_email_format
        CHECK (email ~* '^[^@\s]+@[^@\s]+\.[^@\s]+$'),
    CONSTRAINT chk_users_email_lowercase
        CHECK (email = lower(email)),
    CONSTRAINT chk_users_status_deactivated_at_consistency
        CHECK ((status = 'deactivated') = (deactivated_at IS NOT NULL)),
    CONSTRAINT chk_users_status_first_login_consistency
        CHECK ((status = 'pending') = (first_logged_in_at IS NULL))
);

-- auth.service_accounts --------------------------------------------------------
-- Admin-issued API keys for external read-only scripts (/api/ext/v1). key_hash
-- is the SHA-256 of the pak_* secret (high-entropy, so a fast hash suffices);
-- the plaintext is shown once at creation and never stored. id is an
-- app-supplied UUIDv7 (no DB default).
CREATE TABLE auth.service_accounts (
    id          UUID        NOT NULL,
    name        TEXT        NOT NULL,
    key_hash    BYTEA       NOT NULL,
    status      auth.service_account_status NOT NULL DEFAULT 'active',
    created_by  UUID        NOT NULL,
    revoked_at  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_service_accounts PRIMARY KEY (id),
    CONSTRAINT uq_service_accounts_key_hash UNIQUE (key_hash),
    CONSTRAINT chk_service_accounts_status_revoked_at_consistency
        CHECK ((status = 'revoked') = (revoked_at IS NOT NULL))
);

-- audit.audit_log ------------------------------------------------------------
-- Append-only, immutability enforced by trg_audit_log_immutable (invariant 5
-- in domain-logic.txt). No FK on actor_user_id or entity_id: audit must
-- survive deletes / deactivations. No updated_at: rows are never edited.
CREATE TABLE audit.audit_log (
    id              UUID        NOT NULL DEFAULT gen_random_uuid(),
    actor_user_id   UUID,
    action          audit.audit_action NOT NULL,
    entity_schema   TEXT        NOT NULL,
    entity_table    TEXT        NOT NULL,
    entity_id       UUID        NOT NULL,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Outbox row that produced this entry; unique so a redelivered projection
    -- deduplicates instead of double-appending.
    event_id        UUID,

    CONSTRAINT pk_audit_log PRIMARY KEY (id),
    CONSTRAINT uq_audit_log_event_id UNIQUE (event_id)
);

-- audit.outbox_events ---------------------------------------------------------
-- Transactional outbox for audited domain events: written in the same
-- transaction as the entity row, projected into audit.audit_log by the workers'
-- poller, then marked processed. Mutable (processed_at), so it deliberately
-- does NOT carry the audit_log immutability trigger.
CREATE TABLE audit.outbox_events (
    id           UUID        NOT NULL,
    topic        TEXT        NOT NULL,
    payload      BYTEA       NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    processed_at TIMESTAMPTZ,

    CONSTRAINT pk_outbox_events PRIMARY KEY (id)
);

-- chat.message_attachments ---------------------------------------------------
-- Trusted metadata for files attached to Scylla chat messages. The Scylla
-- message row keeps only the storage keys; the filename / content-type / size
-- live here, keyed by storage_key, so downloads and the orphan-upload sweep
-- never trust client input or scan Scylla. Write-once; no updated_at. id is an
-- app-supplied UUIDv7 (no DB default).
CREATE TABLE chat.message_attachments (
    id                   UUID        NOT NULL,
    -- Scylla channel id; cross-store reference, intentionally no FK.
    channel_id           UUID        NOT NULL,
    uploaded_by_user_id  UUID        NOT NULL,
    filename             TEXT        NOT NULL,
    content_type         TEXT        NOT NULL,
    size_bytes           BIGINT      NOT NULL,
    storage_key          TEXT        NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_message_attachments PRIMARY KEY (id),
    CONSTRAINT uq_message_attachments_storage_key UNIQUE (storage_key),
    CONSTRAINT chk_message_attachments_size_positive CHECK (size_bytes > 0)
);

-- notification.notifications -------------------------------------------------
-- Write-once for display; only read_at mutates after creation, which is
-- intentionally not driven by updated_at trigger.
CREATE TABLE notification.notifications (
    id                UUID        NOT NULL DEFAULT gen_random_uuid(),
    recipient_user_id UUID        NOT NULL,
    kind              notification.notification_kind NOT NULL,
    payload           JSONB       NOT NULL DEFAULT '{}'::jsonb,
    read_at           TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_notifications PRIMARY KEY (id)
);

-- org.groups -----------------------------------------------------------------
-- Soft delete: archived_at hides the group from active queries; the row stays
-- so history (projects, chats, audit) keeps resolving.
CREATE TABLE org.groups (
    id          UUID        NOT NULL DEFAULT gen_random_uuid(),
    name        TEXT        NOT NULL,
    description TEXT        NOT NULL DEFAULT '',
    kind        org.group_kind NOT NULL DEFAULT 'standard',
    archived_at TIMESTAMPTZ,
    version     BIGINT      NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_groups PRIMARY KEY (id),
    CONSTRAINT uq_groups_name UNIQUE (name)
);

-- org.memberships ------------------------------------------------------------
CREATE TABLE org.memberships (
    id              UUID        NOT NULL DEFAULT gen_random_uuid(),
    group_id        UUID        NOT NULL,
    user_id         UUID        NOT NULL,
    role            org.group_role NOT NULL,
    joined_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deactivated_at  TIMESTAMPTZ,
    version         BIGINT      NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_memberships PRIMARY KEY (id),
    -- invariant 3: a user has at most one role in a given group
    CONSTRAINT uq_memberships_group_id_user_id UNIQUE (group_id, user_id)
);

-- project.projects -----------------------------------------------------------
CREATE TABLE project.projects (
    id                  UUID        NOT NULL DEFAULT gen_random_uuid(),
    owner_group_id      UUID        NOT NULL,
    created_by_user_id  UUID        NOT NULL,
    name                TEXT        NOT NULL,
    description         TEXT        NOT NULL DEFAULT '',
    status              project.project_status NOT NULL DEFAULT 'planning',
    progress            SMALLINT    NOT NULL DEFAULT 0,
    completed_at        TIMESTAMPTZ,
    version             BIGINT      NOT NULL DEFAULT 0,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_projects PRIMARY KEY (id),
    CONSTRAINT chk_projects_progress_range CHECK (progress BETWEEN 0 AND 100),
    -- completed_at is set iff the project is in a terminal-completed state
    CONSTRAINT chk_projects_completed_at_consistency
        CHECK ((status = 'completed') = (completed_at IS NOT NULL))
);

-- project.project_collaborators ----------------------------------------------
CREATE TABLE project.project_collaborators (
    id          UUID        NOT NULL DEFAULT gen_random_uuid(),
    group_id    UUID        NOT NULL,
    project_id  UUID        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_project_collaborators PRIMARY KEY (id),
    CONSTRAINT uq_project_collaborators_project_id_group_id
        UNIQUE (project_id, group_id)
);

-- project.project_invites ----------------------------------------------------
CREATE TABLE project.project_invites (
    id                    UUID        NOT NULL DEFAULT gen_random_uuid(),
    invited_by_user_id    UUID        NOT NULL,
    invited_group_id      UUID        NOT NULL,
    project_id            UUID        NOT NULL,
    responded_by_user_id  UUID,
    status                project.invite_status NOT NULL DEFAULT 'pending',
    responded_at          TIMESTAMPTZ,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_project_invites PRIMARY KEY (id),
    CONSTRAINT chk_project_invites_response_consistency
        CHECK (
            (status IN ('accepted', 'declined') AND responded_by_user_id IS NOT NULL AND responded_at IS NOT NULL)
            OR (status IN ('pending', 'revoked'))
        )
);

-- project.requests -----------------------------------------------------------
CREATE TABLE project.requests (
    id                UUID        NOT NULL DEFAULT gen_random_uuid(),
    assignee_user_id  UUID,
    creator_user_id   UUID        NOT NULL,
    project_id        UUID        NOT NULL,
    title             TEXT        NOT NULL,
    description       TEXT        NOT NULL DEFAULT '',
    status            project.request_status NOT NULL DEFAULT 'draft',
    priority          project.request_priority NOT NULL DEFAULT 'normal',
    progress          SMALLINT    NOT NULL DEFAULT 0,
    due_at            TIMESTAMPTZ,
    completed_at      TIMESTAMPTZ,
    version           BIGINT      NOT NULL DEFAULT 0,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_requests PRIMARY KEY (id),
    CONSTRAINT chk_requests_progress_range CHECK (progress BETWEEN 0 AND 100),
    -- A request beyond 'submitted' must have an assignee
    CONSTRAINT chk_requests_assignee_required_after_submitted
        CHECK (status IN ('draft', 'submitted', 'cancelled') OR assignee_user_id IS NOT NULL),
    -- completed_at is set iff the request reached 'completed'
    CONSTRAINT chk_requests_completed_at_consistency
        CHECK ((status = 'completed') = (completed_at IS NOT NULL))
);

-- project.request_attachments ------------------------------------------------
-- Write-once metadata referencing a MinIO object. No updated_at.
CREATE TABLE project.request_attachments (
    id                   UUID        NOT NULL DEFAULT gen_random_uuid(),
    request_id           UUID        NOT NULL,
    uploaded_by_user_id  UUID        NOT NULL,
    filename             TEXT        NOT NULL,
    content_type         TEXT        NOT NULL,
    size_bytes           BIGINT      NOT NULL,
    storage_key          TEXT        NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_request_attachments PRIMARY KEY (id),
    CONSTRAINT uq_request_attachments_storage_key UNIQUE (storage_key),
    CONSTRAINT chk_request_attachments_size_positive CHECK (size_bytes > 0)
);

-- project.request_comments ---------------------------------------------------
-- Discussion comments on a request. Sibling of ticket.ticket_comments (real
-- FKs, nothing polymorphic) behind one domain model. id is an app-supplied
-- UUIDv7 (time-ordered), so (request_id, id DESC) doubles as the pagination
-- index. edited_at is set by the app on edit; no updated_at trigger.
CREATE TABLE project.request_comments (
    id              UUID        NOT NULL,
    request_id      UUID        NOT NULL,
    author_user_id  UUID        NOT NULL,
    body            TEXT        NOT NULL,
    edited_at       TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_request_comments PRIMARY KEY (id),
    CONSTRAINT chk_request_comments_body_not_empty CHECK (length(btrim(body)) > 0)
);

-- reporting.reports ----------------------------------------------------------
-- Archive of generated report artifacts (monthly scheduled + on-demand). The
-- PDF payload lives in MinIO under storage_key; only metadata is here.
-- Write-once; no updated_at. id is an app-supplied UUIDv7 (no DB default).
CREATE TABLE reporting.reports (
    id            UUID        NOT NULL,
    kind          reporting.report_kind  NOT NULL,
    scope         reporting.report_scope NOT NULL,
    group_id      UUID,                       -- NOT NULL iff scope = 'group'
    subject_user_id UUID,                     -- NOT NULL iff scope = 'staff'
    period_start  TIMESTAMPTZ NOT NULL,       -- inclusive (first instant of month / Jan 1)
    period_end    TIMESTAMPTZ NOT NULL,       -- exclusive (first instant of next period)
    storage_key   TEXT        NOT NULL,
    content_type  TEXT        NOT NULL DEFAULT 'application/pdf',
    size_bytes    BIGINT      NOT NULL,
    generated_by  UUID,                       -- NULL for the scheduled job
    generated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_reports PRIMARY KEY (id),
    CONSTRAINT uq_reports_storage_key UNIQUE (storage_key),
    CONSTRAINT chk_reports_size_positive CHECK (size_bytes > 0),
    CONSTRAINT chk_reports_period_order CHECK (period_end > period_start),
    CONSTRAINT chk_reports_scope_group_consistency
        CHECK ((scope = 'group') = (group_id IS NOT NULL)),
    CONSTRAINT chk_reports_scope_subject_consistency
        CHECK ((scope = 'staff') = (subject_user_id IS NOT NULL))
);

-- ticket.tickets -------------------------------------------------------------
CREATE TABLE ticket.tickets (
    id                 UUID        NOT NULL DEFAULT gen_random_uuid(),
    assignee_user_id   UUID,
    requester_user_id  UUID        NOT NULL,
    title              TEXT        NOT NULL,
    description        TEXT        NOT NULL DEFAULT '',
    status             ticket.ticket_status NOT NULL DEFAULT 'open',
    priority           ticket.ticket_priority,
    category           ticket.ticket_category NOT NULL,
    triaged_at         TIMESTAMPTZ,
    resolved_at        TIMESTAMPTZ,
    closed_at          TIMESTAMPTZ,
    version            BIGINT      NOT NULL DEFAULT 0,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_tickets PRIMARY KEY (id),
    -- Once triaged, priority is required (set during triage per the doc)
    CONSTRAINT chk_tickets_priority_required_after_open
        CHECK (status = 'open' OR priority IS NOT NULL),
    -- Status / timestamp consistency
    CONSTRAINT chk_tickets_triaged_at_consistency
        CHECK (status = 'open' OR triaged_at IS NOT NULL),
    CONSTRAINT chk_tickets_closed_at_consistency
        CHECK ((status = 'closed') = (closed_at IS NOT NULL))
);

-- ticket.ticket_comments -----------------------------------------------------
-- Discussion comments on a ticket. Sibling of project.request_comments. id is
-- an app-supplied UUIDv7, so (ticket_id, id DESC) doubles as the pagination
-- index. edited_at is set by the app on edit; no updated_at trigger.
CREATE TABLE ticket.ticket_comments (
    id              UUID        NOT NULL,
    ticket_id       UUID        NOT NULL,
    author_user_id  UUID        NOT NULL,
    body            TEXT        NOT NULL,
    edited_at       TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_ticket_comments PRIMARY KEY (id),
    CONSTRAINT chk_ticket_comments_body_not_empty CHECK (length(btrim(body)) > 0)
);


-- attendance.policy ----------------------------------------------------------
-- Typed, validated singleton holding every tunable attendance limit. Edited by
-- HR / Director; loaded into a cached provider at boot. id is a fixed boolean
-- so at most one row exists. Durations are NUMERIC hours; window bounds are TIME.
CREATE TABLE attendance.policy (
    id                            BOOLEAN     NOT NULL DEFAULT true,
    workday_start                 TIME        NOT NULL DEFAULT '08:00',
    work_hours_per_day            NUMERIC(4,2) NOT NULL DEFAULT 8,
    flex_core_start               TIME        NOT NULL DEFAULT '10:00',
    flex_core_end                 TIME        NOT NULL DEFAULT '15:00',
    flex_daily_min                NUMERIC(4,2) NOT NULL DEFAULT 4,
    flex_daily_max                NUMERIC(4,2) NOT NULL DEFAULT 10,
    flex_earliest_start           TIME        NOT NULL DEFAULT '08:00',
    flex_latest_end               TIME        NOT NULL DEFAULT '20:00',
    flex_max_segments             SMALLINT    NOT NULL DEFAULT 2,
    flex_max_per_month            SMALLINT    NOT NULL DEFAULT 5,
    overtime_max_hours_per_month  NUMERIC(5,2) NOT NULL DEFAULT 40,
    balance_carry_years           SMALLINT    NOT NULL DEFAULT 3,
    balance_expiry_policy         attendance.balance_expiry_policy NOT NULL DEFAULT 'warn',
    balance_expiry_warn_days      SMALLINT    NOT NULL DEFAULT 60,
    updated_by_user_id            UUID,
    updated_at                    TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_policy PRIMARY KEY (id),
    CONSTRAINT chk_policy_singleton CHECK (id),
    CONSTRAINT chk_policy_work_hours_positive CHECK (work_hours_per_day > 0),
    CONSTRAINT chk_policy_flex_core_order CHECK (flex_core_start < flex_core_end),
    CONSTRAINT chk_policy_flex_daily_band CHECK (flex_daily_min <= flex_daily_max),
    CONSTRAINT chk_policy_flex_envelope CHECK (flex_earliest_start <= flex_latest_end),
    CONSTRAINT chk_policy_flex_max_segments CHECK (flex_max_segments BETWEEN 1 AND 4),
    CONSTRAINT chk_policy_flex_max_per_month CHECK (flex_max_per_month >= 0),
    CONSTRAINT chk_policy_overtime_cap_positive CHECK (overtime_max_hours_per_month > 0),
    CONSTRAINT chk_policy_carry_years CHECK (balance_carry_years >= 1),
    CONSTRAINT chk_policy_warn_days CHECK (balance_expiry_warn_days >= 0)
);

-- attendance.daily_reports ---------------------------------------------------
-- One per (user, date). Free-text summary plus typed entries (below).
CREATE TABLE attendance.daily_reports (
    id                   UUID        NOT NULL DEFAULT gen_random_uuid(),
    user_id              UUID        NOT NULL,
    report_date          DATE        NOT NULL,
    status               attendance.daily_report_status NOT NULL DEFAULT 'draft',
    summary              TEXT        NOT NULL DEFAULT '',
    submitted_at         TIMESTAMPTZ,
    reviewed_by_user_id  UUID,
    reviewed_at          TIMESTAMPTZ,
    review_note          TEXT        NOT NULL DEFAULT '',
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_daily_reports PRIMARY KEY (id),
    CONSTRAINT uq_daily_reports_user_date UNIQUE (user_id, report_date)
);

-- attendance.daily_report_entries --------------------------------------------
-- A single line of a daily report. request_work entries link a request and may
-- bump its progress; learning / other are free text. Write-once (replaced
-- wholesale when the parent draft is re-saved); no updated_at.
CREATE TABLE attendance.daily_report_entries (
    id               UUID        NOT NULL DEFAULT gen_random_uuid(),
    daily_report_id  UUID        NOT NULL,
    kind             attendance.daily_report_entry_kind NOT NULL,
    description      TEXT        NOT NULL DEFAULT '',
    request_id       UUID,
    hours            NUMERIC(4,2),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_daily_report_entries PRIMARY KEY (id),
    CONSTRAINT chk_daily_report_entries_request_work_has_request
        CHECK (kind <> 'request_work' OR request_id IS NOT NULL),
    CONSTRAINT chk_daily_report_entries_hours_non_negative
        CHECK (hours IS NULL OR hours >= 0)
);

-- attendance.leave_grants ----------------------------------------------------
-- HR-granted yearly leave entitlement. Unit is half a day. Carries up to
-- policy.balance_carry_years years, then expires (FIFO oldest-first).
CREATE TABLE attendance.leave_grants (
    id                  UUID        NOT NULL DEFAULT gen_random_uuid(),
    user_id             UUID        NOT NULL,
    grant_year          SMALLINT    NOT NULL,
    days_granted        NUMERIC(4,1) NOT NULL,
    days_remaining      NUMERIC(4,1) NOT NULL,
    expires_on          DATE        NOT NULL,
    created_by_user_id  UUID,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_leave_grants PRIMARY KEY (id),
    CONSTRAINT uq_leave_grants_user_year UNIQUE (user_id, grant_year),
    CONSTRAINT chk_leave_grants_remaining_range
        CHECK (days_remaining BETWEEN 0 AND days_granted),
    CONSTRAINT chk_leave_grants_granted_half_step
        CHECK ((days_granted * 2) = floor(days_granted * 2)),
    CONSTRAINT chk_leave_grants_remaining_half_step
        CHECK ((days_remaining * 2) = floor(days_remaining * 2))
);

-- attendance.leave_transactions ----------------------------------------------
-- Immutable ledger of every balance movement (grant / consume / refund /
-- adjust / expire). work_pct is recorded on expire when policy says so.
CREATE TABLE attendance.leave_transactions (
    id                  UUID        NOT NULL DEFAULT gen_random_uuid(),
    user_id             UUID        NOT NULL,
    grant_id            UUID        NOT NULL,
    kind                attendance.leave_txn_kind NOT NULL,
    delta               NUMERIC(4,1) NOT NULL,
    dayoff_id           UUID,
    work_pct            NUMERIC(5,2),
    reason              TEXT        NOT NULL DEFAULT '',
    created_by_user_id  UUID,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_leave_transactions PRIMARY KEY (id)
);

-- attendance.holidays --------------------------------------------------------
-- HR-maintained public holiday calendar. Excluded (with weekends) from leave
-- day-counting and the work-percentage denominator.
CREATE TABLE attendance.holidays (
    holiday_date        DATE        NOT NULL,
    name                TEXT        NOT NULL,
    created_by_user_id  UUID,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_holidays PRIMARY KEY (holiday_date),
    CONSTRAINT chk_holidays_name_not_empty CHECK (length(btrim(name)) > 0)
);

-- attendance.dayoff ----------------------------------------------------------
-- Leave request. annual_leave consumes balance and needs leader + HR; sick /
-- unpaid are leader-only and backdatable; remote / other are leader-only.
CREATE TABLE attendance.dayoff (
    id                  UUID        NOT NULL DEFAULT gen_random_uuid(),
    requester_user_id   UUID        NOT NULL,
    kind                attendance.dayoff_kind NOT NULL,
    start_date          DATE        NOT NULL,
    end_date            DATE        NOT NULL,
    start_half          BOOLEAN     NOT NULL DEFAULT false,
    end_half            BOOLEAN     NOT NULL DEFAULT false,
    days                NUMERIC(4,1) NOT NULL,
    reason              TEXT        NOT NULL DEFAULT '',
    status              attendance.dayoff_status NOT NULL DEFAULT 'pending',
    leader_user_id      UUID,
    leader_decided_at   TIMESTAMPTZ,
    hr_user_id          UUID,
    hr_decided_at       TIMESTAMPTZ,
    decision_note       TEXT        NOT NULL DEFAULT '',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_dayoff PRIMARY KEY (id),
    CONSTRAINT chk_dayoff_date_order CHECK (end_date >= start_date),
    CONSTRAINT chk_dayoff_days_non_negative CHECK (days >= 0)
);

-- attendance.overtime --------------------------------------------------------
-- Extra-hours request, leader + HR approval, capped monthly by policy.
CREATE TABLE attendance.overtime (
    id                  UUID        NOT NULL DEFAULT gen_random_uuid(),
    requester_user_id   UUID        NOT NULL,
    work_date           DATE        NOT NULL,
    hours               NUMERIC(4,2) NOT NULL,
    reason              TEXT        NOT NULL DEFAULT '',
    status              attendance.overtime_status NOT NULL DEFAULT 'pending',
    leader_user_id      UUID,
    leader_decided_at   TIMESTAMPTZ,
    hr_user_id          UUID,
    hr_decided_at       TIMESTAMPTZ,
    decision_note       TEXT        NOT NULL DEFAULT '',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_overtime PRIMARY KEY (id),
    CONSTRAINT chk_overtime_hours_positive CHECK (hours > 0)
);

-- attendance.flex_hours ------------------------------------------------------
-- Per-day custom schedule (segments below), leader-approved, capped monthly by
-- policy and reconciled to the monthly expected total.
CREATE TABLE attendance.flex_hours (
    id                  UUID        NOT NULL DEFAULT gen_random_uuid(),
    user_id             UUID        NOT NULL,
    work_date           DATE        NOT NULL,
    status              attendance.flex_status NOT NULL DEFAULT 'pending',
    leader_user_id      UUID,
    decided_at          TIMESTAMPTZ,
    decision_note       TEXT        NOT NULL DEFAULT '',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_flex_hours PRIMARY KEY (id),
    CONSTRAINT uq_flex_hours_user_date UNIQUE (user_id, work_date)
);

-- attendance.flex_segments ---------------------------------------------------
-- Up to N (policy.flex_max_segments) ordered work blocks for one flex day.
-- Replaced wholesale when the parent flex request is re-saved; no updated_at.
CREATE TABLE attendance.flex_segments (
    id          UUID        NOT NULL DEFAULT gen_random_uuid(),
    flex_id     UUID        NOT NULL,
    seq         SMALLINT    NOT NULL,
    start_at    TIME        NOT NULL,
    end_at      TIME        NOT NULL,

    CONSTRAINT pk_flex_segments PRIMARY KEY (id),
    CONSTRAINT uq_flex_segments_flex_seq UNIQUE (flex_id, seq),
    CONSTRAINT chk_flex_segments_time_order CHECK (end_at > start_at)
);


-- -----------------------------------------------------------------------------
-- 6. Foreign key constraints
--    All FKs as separate ALTER TABLE statements (per conventions). Default to
--    ON DELETE RESTRICT: deactivation is soft, so cascading deletes would
--    contradict the lifecycle rules in domain-logic.txt.
-- -----------------------------------------------------------------------------

-- auth.service_accounts
ALTER TABLE auth.service_accounts
    ADD CONSTRAINT fk_service_accounts_created_by
    FOREIGN KEY (created_by) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- org.memberships
ALTER TABLE org.memberships
    ADD CONSTRAINT fk_memberships_group_id
    FOREIGN KEY (group_id) REFERENCES org.groups (id)
    ON DELETE RESTRICT;

ALTER TABLE org.memberships
    ADD CONSTRAINT fk_memberships_user_id
    FOREIGN KEY (user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- project.projects
ALTER TABLE project.projects
    ADD CONSTRAINT fk_projects_owner_group_id
    FOREIGN KEY (owner_group_id) REFERENCES org.groups (id)
    ON DELETE RESTRICT;

ALTER TABLE project.projects
    ADD CONSTRAINT fk_projects_created_by_user_id
    FOREIGN KEY (created_by_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- project.project_collaborators
ALTER TABLE project.project_collaborators
    ADD CONSTRAINT fk_project_collaborators_project_id
    FOREIGN KEY (project_id) REFERENCES project.projects (id)
    ON DELETE CASCADE;

ALTER TABLE project.project_collaborators
    ADD CONSTRAINT fk_project_collaborators_group_id
    FOREIGN KEY (group_id) REFERENCES org.groups (id)
    ON DELETE RESTRICT;

-- project.project_invites
ALTER TABLE project.project_invites
    ADD CONSTRAINT fk_project_invites_project_id
    FOREIGN KEY (project_id) REFERENCES project.projects (id)
    ON DELETE CASCADE;

ALTER TABLE project.project_invites
    ADD CONSTRAINT fk_project_invites_invited_group_id
    FOREIGN KEY (invited_group_id) REFERENCES org.groups (id)
    ON DELETE RESTRICT;

ALTER TABLE project.project_invites
    ADD CONSTRAINT fk_project_invites_invited_by_user_id
    FOREIGN KEY (invited_by_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

ALTER TABLE project.project_invites
    ADD CONSTRAINT fk_project_invites_responded_by_user_id
    FOREIGN KEY (responded_by_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- project.requests
ALTER TABLE project.requests
    ADD CONSTRAINT fk_requests_project_id
    FOREIGN KEY (project_id) REFERENCES project.projects (id)
    ON DELETE CASCADE;

ALTER TABLE project.requests
    ADD CONSTRAINT fk_requests_creator_user_id
    FOREIGN KEY (creator_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

ALTER TABLE project.requests
    ADD CONSTRAINT fk_requests_assignee_user_id
    FOREIGN KEY (assignee_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- project.request_attachments
ALTER TABLE project.request_attachments
    ADD CONSTRAINT fk_request_attachments_request_id
    FOREIGN KEY (request_id) REFERENCES project.requests (id)
    ON DELETE CASCADE;

ALTER TABLE project.request_attachments
    ADD CONSTRAINT fk_request_attachments_uploaded_by_user_id
    FOREIGN KEY (uploaded_by_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- ticket.tickets
ALTER TABLE ticket.tickets
    ADD CONSTRAINT fk_tickets_requester_user_id
    FOREIGN KEY (requester_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

ALTER TABLE ticket.tickets
    ADD CONSTRAINT fk_tickets_assignee_user_id
    FOREIGN KEY (assignee_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- reporting.reports
ALTER TABLE reporting.reports
    ADD CONSTRAINT fk_reports_group_id
    FOREIGN KEY (group_id) REFERENCES org.groups (id)
    ON DELETE RESTRICT;

ALTER TABLE reporting.reports
    ADD CONSTRAINT fk_reports_subject_user_id
    FOREIGN KEY (subject_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

ALTER TABLE reporting.reports
    ADD CONSTRAINT fk_reports_generated_by
    FOREIGN KEY (generated_by) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- notification.notifications
ALTER TABLE notification.notifications
    ADD CONSTRAINT fk_notifications_recipient_user_id
    FOREIGN KEY (recipient_user_id) REFERENCES auth.users (id)
    ON DELETE CASCADE;

-- chat.message_attachments
ALTER TABLE chat.message_attachments
    ADD CONSTRAINT fk_message_attachments_uploaded_by_user_id
    FOREIGN KEY (uploaded_by_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- project.request_comments
ALTER TABLE project.request_comments
    ADD CONSTRAINT fk_request_comments_request_id
    FOREIGN KEY (request_id) REFERENCES project.requests (id)
    ON DELETE CASCADE;

ALTER TABLE project.request_comments
    ADD CONSTRAINT fk_request_comments_author_user_id
    FOREIGN KEY (author_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- ticket.ticket_comments
ALTER TABLE ticket.ticket_comments
    ADD CONSTRAINT fk_ticket_comments_ticket_id
    FOREIGN KEY (ticket_id) REFERENCES ticket.tickets (id)
    ON DELETE CASCADE;

ALTER TABLE ticket.ticket_comments
    ADD CONSTRAINT fk_ticket_comments_author_user_id
    FOREIGN KEY (author_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;


-- attendance.policy
ALTER TABLE attendance.policy
    ADD CONSTRAINT fk_policy_updated_by_user_id
    FOREIGN KEY (updated_by_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- attendance.daily_reports
ALTER TABLE attendance.daily_reports
    ADD CONSTRAINT fk_daily_reports_user_id
    FOREIGN KEY (user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

ALTER TABLE attendance.daily_reports
    ADD CONSTRAINT fk_daily_reports_reviewed_by_user_id
    FOREIGN KEY (reviewed_by_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- attendance.daily_report_entries
ALTER TABLE attendance.daily_report_entries
    ADD CONSTRAINT fk_daily_report_entries_daily_report_id
    FOREIGN KEY (daily_report_id) REFERENCES attendance.daily_reports (id)
    ON DELETE CASCADE;

ALTER TABLE attendance.daily_report_entries
    ADD CONSTRAINT fk_daily_report_entries_request_id
    FOREIGN KEY (request_id) REFERENCES project.requests (id)
    ON DELETE RESTRICT;

-- attendance.leave_grants
ALTER TABLE attendance.leave_grants
    ADD CONSTRAINT fk_leave_grants_user_id
    FOREIGN KEY (user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

ALTER TABLE attendance.leave_grants
    ADD CONSTRAINT fk_leave_grants_created_by_user_id
    FOREIGN KEY (created_by_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- attendance.leave_transactions
ALTER TABLE attendance.leave_transactions
    ADD CONSTRAINT fk_leave_transactions_user_id
    FOREIGN KEY (user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

ALTER TABLE attendance.leave_transactions
    ADD CONSTRAINT fk_leave_transactions_grant_id
    FOREIGN KEY (grant_id) REFERENCES attendance.leave_grants (id)
    ON DELETE CASCADE;

ALTER TABLE attendance.leave_transactions
    ADD CONSTRAINT fk_leave_transactions_dayoff_id
    FOREIGN KEY (dayoff_id) REFERENCES attendance.dayoff (id)
    ON DELETE RESTRICT;

ALTER TABLE attendance.leave_transactions
    ADD CONSTRAINT fk_leave_transactions_created_by_user_id
    FOREIGN KEY (created_by_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- attendance.holidays
ALTER TABLE attendance.holidays
    ADD CONSTRAINT fk_holidays_created_by_user_id
    FOREIGN KEY (created_by_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- attendance.dayoff
ALTER TABLE attendance.dayoff
    ADD CONSTRAINT fk_dayoff_requester_user_id
    FOREIGN KEY (requester_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

ALTER TABLE attendance.dayoff
    ADD CONSTRAINT fk_dayoff_leader_user_id
    FOREIGN KEY (leader_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

ALTER TABLE attendance.dayoff
    ADD CONSTRAINT fk_dayoff_hr_user_id
    FOREIGN KEY (hr_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- attendance.overtime
ALTER TABLE attendance.overtime
    ADD CONSTRAINT fk_overtime_requester_user_id
    FOREIGN KEY (requester_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

ALTER TABLE attendance.overtime
    ADD CONSTRAINT fk_overtime_leader_user_id
    FOREIGN KEY (leader_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

ALTER TABLE attendance.overtime
    ADD CONSTRAINT fk_overtime_hr_user_id
    FOREIGN KEY (hr_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- attendance.flex_hours
ALTER TABLE attendance.flex_hours
    ADD CONSTRAINT fk_flex_hours_user_id
    FOREIGN KEY (user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

ALTER TABLE attendance.flex_hours
    ADD CONSTRAINT fk_flex_hours_leader_user_id
    FOREIGN KEY (leader_user_id) REFERENCES auth.users (id)
    ON DELETE RESTRICT;

-- attendance.flex_segments
ALTER TABLE attendance.flex_segments
    ADD CONSTRAINT fk_flex_segments_flex_id
    FOREIGN KEY (flex_id) REFERENCES attendance.flex_hours (id)
    ON DELETE CASCADE;


-- -----------------------------------------------------------------------------
-- 7. Indexes
--    Every FK column is indexed (Postgres does not auto-index FKs).
--    Additional partial / composite indexes target known query patterns.
-- -----------------------------------------------------------------------------

-- auth.users
CREATE INDEX idx_users_status_active
    ON auth.users (status)
    WHERE status = 'active';

-- auth.service_accounts
CREATE INDEX idx_service_accounts_created_by
    ON auth.service_accounts (created_by);

-- key rotation: only active accounts contend for a name; a revoked one frees it
CREATE UNIQUE INDEX uq_service_accounts_active_name
    ON auth.service_accounts (name)
    WHERE status = 'active';

-- org.groups
-- invariant: at most one IT group at any time
CREATE UNIQUE INDEX uq_groups_one_it
    ON org.groups (kind)
    WHERE kind = 'it';

-- org.memberships
CREATE INDEX idx_memberships_group_id
    ON org.memberships (group_id);

CREATE INDEX idx_memberships_user_id_active
    ON org.memberships (user_id)
    WHERE deactivated_at IS NULL;

-- invariant 1: exactly one active leader per group
CREATE UNIQUE INDEX uq_memberships_one_leader_per_group
    ON org.memberships (group_id)
    WHERE role = 'leader' AND deactivated_at IS NULL;

-- project.projects
CREATE INDEX idx_projects_owner_group_id_status
    ON project.projects (owner_group_id, status);

CREATE INDEX idx_projects_created_by_user_id
    ON project.projects (created_by_user_id);

-- project.project_collaborators
CREATE INDEX idx_project_collaborators_group_id
    ON project.project_collaborators (group_id);

-- project.project_invites
CREATE INDEX idx_project_invites_invited_group_id_status
    ON project.project_invites (invited_group_id, status);

CREATE INDEX idx_project_invites_invited_by_user_id
    ON project.project_invites (invited_by_user_id);

CREATE INDEX idx_project_invites_responded_by_user_id
    ON project.project_invites (responded_by_user_id)
    WHERE responded_by_user_id IS NOT NULL;

-- doc rule: cannot send a second pending invite for the same (project, group).
-- Re-invite allowed once a prior invite is declined / revoked.
CREATE UNIQUE INDEX uq_project_invites_pending_per_project_group
    ON project.project_invites (project_id, invited_group_id)
    WHERE status = 'pending';

-- project.requests
CREATE INDEX idx_requests_project_id_status
    ON project.requests (project_id, status);

CREATE INDEX idx_requests_creator_user_id
    ON project.requests (creator_user_id);

CREATE INDEX idx_requests_assignee_user_id_status
    ON project.requests (assignee_user_id, status)
    WHERE assignee_user_id IS NOT NULL;

-- project.request_attachments
CREATE INDEX idx_request_attachments_request_id
    ON project.request_attachments (request_id);

CREATE INDEX idx_request_attachments_uploaded_by_user_id
    ON project.request_attachments (uploaded_by_user_id);

-- ticket.tickets
CREATE INDEX idx_tickets_requester_user_id
    ON ticket.tickets (requester_user_id);

CREATE INDEX idx_tickets_assignee_user_id
    ON ticket.tickets (assignee_user_id)
    WHERE assignee_user_id IS NOT NULL;

-- IT triage queue: hot path is "open / triaged / assigned / in_progress
-- by priority". Partial index keeps it tight.
CREATE INDEX idx_tickets_status_priority_open
    ON ticket.tickets (status, priority)
    WHERE status IN ('open', 'triaged', 'assigned', 'in_progress', 'reopened');

-- reporting.reports
-- Browse "latest reports of a kind" newest-first.
CREATE INDEX idx_reports_kind_period
    ON reporting.reports (kind, period_start DESC);

CREATE INDEX idx_reports_group_id_period
    ON reporting.reports (group_id, period_start DESC)
    WHERE group_id IS NOT NULL;

-- Idempotency for the scheduler: at most one company report per (kind, period),
-- one per-group report per (kind, group, period), and one per-staff report per
-- (kind, subject, period). Split because group_id / subject_user_id are NULL
-- outside their scope (NULLs are distinct in a plain unique index).
CREATE UNIQUE INDEX uq_reports_company_period
    ON reporting.reports (kind, period_start)
    WHERE scope = 'company';

CREATE UNIQUE INDEX uq_reports_group_period
    ON reporting.reports (kind, group_id, period_start)
    WHERE scope = 'group';

CREATE UNIQUE INDEX uq_reports_staff_period
    ON reporting.reports (kind, subject_user_id, period_start)
    WHERE scope = 'staff';

-- notification.notifications
CREATE INDEX idx_notifications_recipient_user_id_unread
    ON notification.notifications (recipient_user_id, created_at DESC)
    WHERE read_at IS NULL;

CREATE INDEX idx_notifications_recipient_user_id_created
    ON notification.notifications (recipient_user_id, created_at DESC);

-- audit.audit_log
CREATE INDEX idx_audit_log_entity
    ON audit.audit_log (entity_schema, entity_table, entity_id, occurred_at DESC);

-- audit.outbox_events
CREATE INDEX idx_outbox_events_unprocessed
    ON audit.outbox_events (created_at) WHERE processed_at IS NULL;

CREATE INDEX idx_audit_log_actor_user_id_occurred
    ON audit.audit_log (actor_user_id, occurred_at DESC)
    WHERE actor_user_id IS NOT NULL;

-- Global admin feed (list_recent): newest-first scan / occurred_at cursor.
CREATE INDEX idx_audit_log_occurred
    ON audit.audit_log (occurred_at DESC);

-- chat.message_attachments
CREATE INDEX idx_message_attachments_channel_id
    ON chat.message_attachments (channel_id);

CREATE INDEX idx_message_attachments_uploaded_by_user_id
    ON chat.message_attachments (uploaded_by_user_id);

-- project.request_comments
-- (request_id, id DESC) doubles as the (request_id, created_at DESC) page index.
CREATE INDEX idx_request_comments_request_id_id
    ON project.request_comments (request_id, id DESC);

CREATE INDEX idx_request_comments_author_user_id
    ON project.request_comments (author_user_id);

-- ticket.ticket_comments
CREATE INDEX idx_ticket_comments_ticket_id_id
    ON ticket.ticket_comments (ticket_id, id DESC);

CREATE INDEX idx_ticket_comments_author_user_id
    ON ticket.ticket_comments (author_user_id);


-- attendance.daily_reports
CREATE INDEX idx_daily_reports_user_id_date
    ON attendance.daily_reports (user_id, report_date DESC);

CREATE INDEX idx_daily_reports_reviewed_by_user_id
    ON attendance.daily_reports (reviewed_by_user_id)
    WHERE reviewed_by_user_id IS NOT NULL;

-- attendance.daily_report_entries
CREATE INDEX idx_daily_report_entries_daily_report_id
    ON attendance.daily_report_entries (daily_report_id);

CREATE INDEX idx_daily_report_entries_request_id
    ON attendance.daily_report_entries (request_id)
    WHERE request_id IS NOT NULL;

-- attendance.leave_grants
CREATE INDEX idx_leave_grants_user_id_expires
    ON attendance.leave_grants (user_id, expires_on);

CREATE INDEX idx_leave_grants_created_by_user_id
    ON attendance.leave_grants (created_by_user_id)
    WHERE created_by_user_id IS NOT NULL;

-- attendance.leave_transactions
CREATE INDEX idx_leave_transactions_user_id_created
    ON attendance.leave_transactions (user_id, created_at DESC);

CREATE INDEX idx_leave_transactions_grant_id
    ON attendance.leave_transactions (grant_id);

CREATE INDEX idx_leave_transactions_dayoff_id
    ON attendance.leave_transactions (dayoff_id)
    WHERE dayoff_id IS NOT NULL;

-- One consume row per (dayoff, grant): permits a single FIFO consume spanning
-- multiple grants, but blocks a concurrent duplicate consume of the same dayoff
-- (the second insert collides -> UniqueViolation -> Conflict), making the leave
-- ledger debit concurrency-safe alongside the application-level idempotency guard.
CREATE UNIQUE INDEX uq_leave_transactions_consume_per_grant
    ON attendance.leave_transactions (dayoff_id, grant_id)
    WHERE kind = 'consume' AND dayoff_id IS NOT NULL;

-- attendance.dayoff
CREATE INDEX idx_dayoff_requester_user_id_status
    ON attendance.dayoff (requester_user_id, status);

CREATE INDEX idx_dayoff_status
    ON attendance.dayoff (status);

CREATE INDEX idx_dayoff_leader_user_id
    ON attendance.dayoff (leader_user_id)
    WHERE leader_user_id IS NOT NULL;

CREATE INDEX idx_dayoff_hr_user_id
    ON attendance.dayoff (hr_user_id)
    WHERE hr_user_id IS NOT NULL;

-- attendance.overtime
CREATE INDEX idx_overtime_requester_user_id_status
    ON attendance.overtime (requester_user_id, status);

CREATE INDEX idx_overtime_status
    ON attendance.overtime (status);

CREATE INDEX idx_overtime_leader_user_id
    ON attendance.overtime (leader_user_id)
    WHERE leader_user_id IS NOT NULL;

CREATE INDEX idx_overtime_hr_user_id
    ON attendance.overtime (hr_user_id)
    WHERE hr_user_id IS NOT NULL;

-- attendance.flex_hours
CREATE INDEX idx_flex_hours_user_id_date
    ON attendance.flex_hours (user_id, work_date);

CREATE INDEX idx_flex_hours_status
    ON attendance.flex_hours (status);

CREATE INDEX idx_flex_hours_leader_user_id
    ON attendance.flex_hours (leader_user_id)
    WHERE leader_user_id IS NOT NULL;

-- attendance.flex_segments
CREATE INDEX idx_flex_segments_flex_id
    ON attendance.flex_segments (flex_id);


-- -----------------------------------------------------------------------------
-- 8. Triggers
--    updated_at maintenance + cross-table invariants. Append-only tables
--    (audit_log, notifications, request_attachments) intentionally have no
--    updated_at trigger.
-- -----------------------------------------------------------------------------

CREATE TRIGGER trg_users_set_updated_at
    BEFORE UPDATE ON auth.users
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_service_accounts_set_updated_at
    BEFORE UPDATE ON auth.service_accounts
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_groups_set_updated_at
    BEFORE UPDATE ON org.groups
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_memberships_set_updated_at
    BEFORE UPDATE ON org.memberships
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_projects_set_updated_at
    BEFORE UPDATE ON project.projects
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_project_collaborators_set_updated_at
    BEFORE UPDATE ON project.project_collaborators
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_project_invites_set_updated_at
    BEFORE UPDATE ON project.project_invites
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_requests_set_updated_at
    BEFORE UPDATE ON project.requests
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_tickets_set_updated_at
    BEFORE UPDATE ON ticket.tickets
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

-- invariant 7: owner group cannot be a collaborator of its own project
CREATE TRIGGER trg_project_collaborators_no_self_collab
    BEFORE INSERT OR UPDATE OF project_id, group_id
    ON project.project_collaborators
    FOR EACH ROW EXECUTE FUNCTION project.fn_no_self_collab();

-- invariant 5: audit log rows can never be updated or deleted
CREATE TRIGGER trg_audit_log_immutable
    BEFORE UPDATE OR DELETE ON audit.audit_log
    FOR EACH ROW EXECUTE FUNCTION audit.fn_forbid_mutation();

-- attendance: updated_at maintenance. Entry / segment / transaction child rows
-- and the holidays calendar are write-once, so they have no updated_at trigger.
CREATE TRIGGER trg_policy_set_updated_at
    BEFORE UPDATE ON attendance.policy
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_daily_reports_set_updated_at
    BEFORE UPDATE ON attendance.daily_reports
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_leave_grants_set_updated_at
    BEFORE UPDATE ON attendance.leave_grants
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_dayoff_set_updated_at
    BEFORE UPDATE ON attendance.dayoff
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_overtime_set_updated_at
    BEFORE UPDATE ON attendance.overtime
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();

CREATE TRIGGER trg_flex_hours_set_updated_at
    BEFORE UPDATE ON attendance.flex_hours
    FOR EACH ROW EXECUTE FUNCTION public.fn_set_updated_at();


-- -----------------------------------------------------------------------------
-- 9. Comments
-- -----------------------------------------------------------------------------

COMMENT ON SCHEMA attendance   IS 'Staff time tracking: tunable policy, daily reports, leave balances/holidays/day-off, overtime, flexible hours.';
COMMENT ON SCHEMA auth         IS 'User identity, profile, lifecycle.';
COMMENT ON SCHEMA audit        IS 'Append-only audit log. Immutable by convention (invariant 5).';
COMMENT ON SCHEMA chat         IS 'Relational sidecar for chat: trusted attachment metadata. Messages/channels live in Scylla.';
COMMENT ON SCHEMA notification IS 'Persisted user-facing notifications. Read fanout is denormalized via the payload JSONB.';
COMMENT ON SCHEMA org          IS 'Organizational structure: groups and memberships.';
COMMENT ON SCHEMA project      IS 'Projects, group collaborations, work requests, attachments.';
COMMENT ON SCHEMA reporting    IS 'Generated report artifacts (metadata only; payload in MinIO).';
COMMENT ON SCHEMA ticket       IS 'IT support tickets. Separate hierarchy from projects.';

COMMENT ON TABLE auth.users IS
    'Application users. Identity row persists forever (deactivation is soft) so historical references stay valid.';
COMMENT ON COLUMN auth.users.password_hash IS
    'Argon2id hash; never store plaintext.';
COMMENT ON COLUMN auth.users.avatar_storage_key IS
    'MinIO object key for the user avatar; NULL until uploaded.';
COMMENT ON COLUMN auth.users.status IS
    'pending = HR-created, not yet logged in; active = normal; deactivated = soft-deleted.';
COMMENT ON COLUMN auth.users.system_role IS
    'Org-wide identity orthogonal to per-group role. NULL for most users; director / hr carry one. IT staff are identified by membership in the group with kind=''it'', not via this column.';
COMMENT ON COLUMN auth.users.first_logged_in_at IS
    'Set on the user''s first successful login; pending<->NULL.';

COMMENT ON TABLE org.groups IS
    'Organizational unit. Flat namespace - no nesting. IT is a normal group flagged via kind=''it''.';
COMMENT ON COLUMN org.groups.kind IS
    'standard | it. Only one group may have kind=it at any time (enforced by partial unique index).';

COMMENT ON TABLE org.memberships IS
    'Links users to groups with a role. A user has at most one role per group (invariant 3). Soft-deleted via deactivated_at to preserve audit shape.';
COMMENT ON COLUMN org.memberships.role IS
    'leader (one per active membership set per group) | sub_leader | member.';

COMMENT ON TABLE project.projects IS
    'Body of work owned by exactly one group. Ownership transfer requires mutual consent of both group leaders (enforced in application).';
COMMENT ON COLUMN project.projects.status IS
    'planning -> active -> {on_hold, completed, cancelled}. Owner-leader-driven transitions.';
COMMENT ON COLUMN project.projects.progress IS
    'Completion percentage (0-100) set manually by group leaders; feeds the reporting roll-up.';
COMMENT ON COLUMN project.projects.completed_at IS
    'Set by the app on the transition into ''completed''; NULL otherwise. Used for period-accurate completion metrics.';

COMMENT ON TABLE reporting.reports IS
    'Archive of generated reports (monthly scheduled + on-demand). storage_key points at the MinIO object; write-once metadata, no updated_at. generated_by is NULL for the scheduled job.';

COMMENT ON TABLE project.project_collaborators IS
    'Group-level collaboration. Owner group cannot also be a collaborator (invariant 7, enforced by trg_project_collaborators_no_self_collab).';

COMMENT ON TABLE project.project_invites IS
    'Group-level project invites. Lifecycle: pending -> {accepted, declined, revoked}. A new invite for the same (project, group) is allowed only after the prior invite leaves pending.';

COMMENT ON TABLE project.requests IS
    'Unit of work within a project. State machine: draft -> submitted -> assigned -> in_progress -> review -> {completed, cancelled}. Reopening a completed request is forbidden; create a new one instead.';

COMMENT ON TABLE project.request_attachments IS
    'Metadata for files attached to a request. Payload lives in MinIO under storage_key.';

COMMENT ON TABLE project.request_comments IS
    'Discussion comments on a request. Sibling of ticket.ticket_comments. edited_at set on edit; rows are not otherwise mutated.';

COMMENT ON TABLE ticket.ticket_comments IS
    'Discussion comments on a ticket. Sibling of project.request_comments.';

COMMENT ON TABLE chat.message_attachments IS
    'Trusted metadata for chat message attachments. Payload lives in MinIO under storage_key; the Scylla message row holds only the keys. No FK to the channel (cross-store).';

COMMENT ON TABLE ticket.tickets IS
    'IT support ticket raised by any active user. Distinct from project.requests: tickets live outside the project hierarchy.';
COMMENT ON COLUMN ticket.tickets.priority IS
    'NULL until the ticket is triaged; required from triaged onward (chk_tickets_priority_required_after_open).';

COMMENT ON TABLE notification.notifications IS
    'Recipient-scoped, write-once notification rows. payload JSONB carries kind-specific fields (no FK so we can fan out fast and survive entity deletes).';

COMMENT ON TABLE audit.audit_log IS
    'Append-only audit trail. actor_user_id and entity_id intentionally have NO foreign keys so audit history survives deactivation and deletes (invariant 5).';


-- -----------------------------------------------------------------------------
-- 10. Bootstrap account
--     One default Director so a freshly-initialised database is immediately
--     loginable (email: admin@portal.local, password: admin123). Idempotent:
--     re-applying the schema or running this section by hand is a no-op once the
--     row exists. password_hash is a precomputed Argon2id PHC string for "admin123" --
--     the parameters are embedded in the hash, so the application's
--     Argon2::default().verify_password accepts it. Dev default; rotate before
--     any non-local deployment. Richer demo data (groups, ~100 employees,
--     projects/tickets) is optional and lives outside this file -- see
--     infra/seed/ (apply with `cargo make seed`).
-- -----------------------------------------------------------------------------
INSERT INTO auth.users (email, password_hash, full_name, status, system_role, first_logged_in_at)
VALUES (
    'admin@portal.local',
    '$argon2id$v=19$m=19456,t=2,p=1$DDkH8BLeMSpBiPE2J7HqCA$Fx9mB5cw4NW/orBxwOv+Z+22t/QWpmLlNb7RY4wWHu4',
    'Portal Admin',
    'active',
    'director',
    NOW()
)
ON CONFLICT (email) DO NOTHING;


-- -----------------------------------------------------------------------------
-- 11. Attendance policy singleton
--     Seed the one-and-only policy row with the documented defaults so the
--     cached provider has something to load before HR edits anything. Every
--     column has a DEFAULT, so a bare insert of the boolean id is enough.
--     Idempotent: a no-op once the row exists.
-- -----------------------------------------------------------------------------
INSERT INTO attendance.policy (id) VALUES (true)
ON CONFLICT (id) DO NOTHING;
