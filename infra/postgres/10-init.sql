-- =============================================================================
-- init.sql
-- Schema for: Internal Portal (single-company, 100-1000 users)
-- Apply:   psql -h localhost -U portal -d portal -f infra/postgres/init.sql
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
CREATE SCHEMA IF NOT EXISTS auth;
CREATE SCHEMA IF NOT EXISTS audit;
CREATE SCHEMA IF NOT EXISTS notification;
CREATE SCHEMA IF NOT EXISTS org;
CREATE SCHEMA IF NOT EXISTS project;
CREATE SCHEMA IF NOT EXISTS ticket;


-- -----------------------------------------------------------------------------
-- 3. Enums (per-schema, alphabetical)
-- -----------------------------------------------------------------------------

-- auth
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
    'transfer',
    'login',
    'logout'
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
    'system'
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
    first_logged_in_at  TIMESTAMPTZ,
    deactivated_at      TIMESTAMPTZ,
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

-- audit.audit_log ------------------------------------------------------------
-- Append-only, immutable by convention (invariant 5 in domain-logic.txt).
-- No FK on actor_user_id or entity_id: audit must survive deletes /
-- deactivations. No updated_at: rows are never edited.
CREATE TABLE audit.audit_log (
    id              UUID        NOT NULL DEFAULT gen_random_uuid(),
    actor_user_id   UUID,
    action          audit.audit_action NOT NULL,
    entity_schema   TEXT        NOT NULL,
    entity_table    TEXT        NOT NULL,
    entity_id       UUID        NOT NULL,
    payload_before  JSONB,
    payload_after   JSONB,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_audit_log PRIMARY KEY (id)
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
CREATE TABLE org.groups (
    id          UUID        NOT NULL DEFAULT gen_random_uuid(),
    name        TEXT        NOT NULL,
    description TEXT        NOT NULL DEFAULT '',
    kind        org.group_kind NOT NULL DEFAULT 'standard',
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
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_projects PRIMARY KEY (id)
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
    due_at            TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pk_requests PRIMARY KEY (id),
    -- A request beyond 'submitted' must have an assignee
    CONSTRAINT chk_requests_assignee_required_after_submitted
        CHECK (status IN ('draft', 'submitted', 'cancelled') OR assignee_user_id IS NOT NULL)
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


-- -----------------------------------------------------------------------------
-- 6. Foreign key constraints
--    All FKs as separate ALTER TABLE statements (per conventions). Default to
--    ON DELETE RESTRICT: deactivation is soft, so cascading deletes would
--    contradict the lifecycle rules in domain-logic.txt.
-- -----------------------------------------------------------------------------

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

-- notification.notifications
ALTER TABLE notification.notifications
    ADD CONSTRAINT fk_notifications_recipient_user_id
    FOREIGN KEY (recipient_user_id) REFERENCES auth.users (id)
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

-- notification.notifications
CREATE INDEX idx_notifications_recipient_user_id_unread
    ON notification.notifications (recipient_user_id, created_at DESC)
    WHERE read_at IS NULL;

CREATE INDEX idx_notifications_recipient_user_id_created
    ON notification.notifications (recipient_user_id, created_at DESC);

-- audit.audit_log
CREATE INDEX idx_audit_log_entity
    ON audit.audit_log (entity_schema, entity_table, entity_id, occurred_at DESC);

CREATE INDEX idx_audit_log_actor_user_id_occurred
    ON audit.audit_log (actor_user_id, occurred_at DESC)
    WHERE actor_user_id IS NOT NULL;

-- Global admin feed (list_recent): newest-first scan / occurred_at cursor.
CREATE INDEX idx_audit_log_occurred
    ON audit.audit_log (occurred_at DESC);


-- -----------------------------------------------------------------------------
-- 8. Triggers
--    updated_at maintenance + cross-table invariants. Append-only tables
--    (audit_log, notifications, request_attachments) intentionally have no
--    updated_at trigger.
-- -----------------------------------------------------------------------------

CREATE TRIGGER trg_users_set_updated_at
    BEFORE UPDATE ON auth.users
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


-- -----------------------------------------------------------------------------
-- 9. Comments
-- -----------------------------------------------------------------------------

COMMENT ON SCHEMA auth         IS 'User identity, profile, lifecycle.';
COMMENT ON SCHEMA audit        IS 'Append-only audit log. Immutable by convention (invariant 5).';
COMMENT ON SCHEMA notification IS 'Persisted user-facing notifications. Read fanout is denormalized via the payload JSONB.';
COMMENT ON SCHEMA org          IS 'Organizational structure: groups and memberships.';
COMMENT ON SCHEMA project      IS 'Projects, group collaborations, work requests, attachments.';
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

COMMENT ON TABLE project.project_collaborators IS
    'Group-level collaboration. Owner group cannot also be a collaborator (invariant 7, enforced by trg_project_collaborators_no_self_collab).';

COMMENT ON TABLE project.project_invites IS
    'Group-level project invites. Lifecycle: pending -> {accepted, declined, revoked}. A new invite for the same (project, group) is allowed only after the prior invite leaves pending.';

COMMENT ON TABLE project.requests IS
    'Unit of work within a project. State machine: draft -> submitted -> assigned -> in_progress -> review -> {completed, cancelled}. Reopening a completed request is forbidden; create a new one instead.';

COMMENT ON TABLE project.request_attachments IS
    'Metadata for files attached to a request. Payload lives in MinIO under storage_key.';

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
--     loginable (email: admin@portal.local, password: 123). Idempotent:
--     re-applying the schema or running this section by hand is a no-op once the
--     row exists. password_hash is a precomputed Argon2id PHC string for "123" --
--     the parameters are embedded in the hash, so the application's
--     Argon2::default().verify_password accepts it. Dev default; rotate before
--     any non-local deployment. Richer demo data (groups, ~100 employees,
--     projects/tickets) is optional and lives outside this file -- see
--     infra/seed/ (apply with `cargo make seed`).
-- -----------------------------------------------------------------------------
INSERT INTO auth.users (email, password_hash, full_name, status, system_role, first_logged_in_at)
VALUES (
    'admin@portal.local',
    '$argon2id$v=19$m=19456,t=2,p=1$S7dTa5Ok0hyf9iKT36EBBw$ZSl6SxhfaAgKS8AqNwpMce1NjjApbWgMRO85deZToxA',
    'Portal Admin',
    'active',
    'director',
    NOW()
)
ON CONFLICT (email) DO NOTHING;
