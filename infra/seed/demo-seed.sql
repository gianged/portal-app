-- =============================================================================
-- demo-seed.sql
-- OPTIONAL simulation data for the Internal Portal (dev/testing only).
--
-- NOT auto-applied. This file lives outside infra/postgres/ on purpose, so the
-- Postgres docker-entrypoint never runs it on a fresh volume. Apply on demand:
--
--   cargo make seed        # applies this file, then materialises OpenFGA tuples
--
-- It adds, on top of the single default Director from 10-init.sql:
--   - hr@portal.local (HR) and it.lead@portal.local (IT-group leader)
--   - 9 groups (one with kind='it')
--   - 100 fake employees
--   - memberships (exactly one active leader per group)
--   - a little sample activity: projects, requests, tickets
--
-- All accounts share the password "admin123" (same Argon2id hash as the default
-- admin). Idempotent: every statement either ON CONFLICT DO NOTHING or guards
-- with NOT EXISTS, so re-running adds nothing. Authorisation tuples are written
-- separately by `seed_authz` (this file only touches Postgres).
-- =============================================================================

\set pw_hash '$argon2id$v=19$m=19456,t=2,p=1$DDkH8BLeMSpBiPE2J7HqCA$Fx9mB5cw4NW/orBxwOv+Z+22t/QWpmLlNb7RY4wWHu4'

BEGIN;

-- -----------------------------------------------------------------------------
-- 1. Privileged accounts (admin already exists from 10-init.sql)
-- -----------------------------------------------------------------------------
INSERT INTO auth.users (email, password_hash, full_name, status, system_role, first_logged_in_at)
VALUES
    ('hr@portal.local',      :'pw_hash', 'Harriet Reyes', 'active', 'hr', NOW()),
    ('it.lead@portal.local', :'pw_hash', 'Ivan Tucker',   'active', NULL, NOW())
ON CONFLICT (email) DO NOTHING;

-- -----------------------------------------------------------------------------
-- 2. Groups (exactly one kind='it')
-- -----------------------------------------------------------------------------
INSERT INTO org.groups (name, description, kind)
VALUES
    ('Executive Office', 'Company leadership and strategy',        'standard'),
    ('People & Culture', 'HR, recruiting, and people operations',  'standard'),
    ('IT Support',       'Internal IT helpdesk and infrastructure','it'),
    ('Engineering',      'Product engineering',                    'standard'),
    ('Product & Design', 'Product management and design',          'standard'),
    ('Sales',            'Revenue and account management',         'standard'),
    ('Marketing',        'Brand, growth, and communications',      'standard'),
    ('Finance & Ops',    'Finance, legal, and operations',         'standard'),
    ('Customer Support', 'Customer success and support',           'standard')
ON CONFLICT (name) DO NOTHING;

-- -----------------------------------------------------------------------------
-- 3. 100 fake employees
--    Set-based: names picked from arrays, uniqueness guaranteed by the trailing
--    index in the email. ~4% are left 'pending' (never-logged-in) for realism.
-- -----------------------------------------------------------------------------
INSERT INTO auth.users (email, password_hash, full_name, status, first_logged_in_at)
SELECT
    lower(n.fn) || '.' || lower(n.ln) || g.i || '@portal.local',
    :'pw_hash',
    n.fn || ' ' || n.ln,
    (CASE WHEN g.i % 25 = 0 THEN 'pending' ELSE 'active' END)::auth.user_status,
    (CASE WHEN g.i % 25 = 0 THEN NULL ELSE NOW() END)
FROM generate_series(1, 100) AS g(i)
CROSS JOIN LATERAL (
    SELECT
        (ARRAY['Ava','Liam','Noah','Emma','Olivia','William','Sophia','James',
               'Isabella','Lucas','Mia','Mason','Charlotte','Ethan','Amelia',
               'Logan','Harper','Elijah','Evelyn','Oliver'])[1 + (g.i % 20)]      AS fn,
        (ARRAY['Nguyen','Smith','Johnson','Tran','Brown','Lee','Garcia','Martinez',
               'Davis','Le','Wilson','Anderson','Pham','Taylor','Thomas','Hoang',
               'Moore','Jackson','Vu','Walker'])[1 + ((g.i / 5) % 20)]            AS ln
) AS n
ON CONFLICT (email) DO NOTHING;

-- -----------------------------------------------------------------------------
-- 4. Memberships
--    4a. Privileged accounts lead their own groups (the sole leader there).
--    4b. Employees are round-robin'd across all 9 groups; within the six
--        non-privileged groups a window function picks exactly one leader
--        (rank 1), one sub-leader (rank 2), the rest members. In the three
--        privileged-led groups employees can only be sub_leader/member, so the
--        one-active-leader-per-group index is never violated. Active users sort
--        first, so leaders are never a 'pending' account.
-- -----------------------------------------------------------------------------

-- 4a.
INSERT INTO org.memberships (group_id, user_id, role)
SELECT g.id, u.id, 'leader'::org.group_role
FROM (VALUES
    ('Executive Office', 'admin@portal.local'),
    ('People & Culture', 'hr@portal.local'),
    ('IT Support',       'it.lead@portal.local')
) AS m(gname, uemail)
JOIN org.groups g  ON g.name  = m.gname
JOIN auth.users u  ON u.email = m.uemail
ON CONFLICT (group_id, user_id) DO NOTHING;

-- 4b.
WITH groups9 AS (
    SELECT name, ord, privileged
    FROM (VALUES
        ('Executive Office', 1, true),
        ('People & Culture', 2, true),
        ('IT Support',       3, true),
        ('Engineering',      4, false),
        ('Product & Design', 5, false),
        ('Sales',            6, false),
        ('Marketing',        7, false),
        ('Finance & Ops',    8, false),
        ('Customer Support', 9, false)
    ) AS t(name, ord, privileged)
),
emp AS (
    SELECT u.id AS user_id,
           (u.status = 'active') AS is_active,
           row_number() OVER (ORDER BY u.email) AS n
    FROM auth.users u
    WHERE u.email LIKE '%@portal.local'
      AND u.email NOT IN ('admin@portal.local', 'hr@portal.local', 'it.lead@portal.local')
),
assigned AS (
    SELECT e.user_id,
           g.name AS group_name,
           g.privileged,
           row_number() OVER (PARTITION BY g.name ORDER BY e.is_active DESC, e.user_id) AS rnk
    FROM emp e
    JOIN groups9 g ON g.ord = 1 + (e.n % 9)
)
INSERT INTO org.memberships (group_id, user_id, role)
SELECT gr.id, a.user_id,
       (CASE
            WHEN a.privileged THEN (CASE WHEN a.rnk = 1 THEN 'sub_leader' ELSE 'member' END)
            ELSE (CASE WHEN a.rnk = 1 THEN 'leader'
                       WHEN a.rnk = 2 THEN 'sub_leader'
                       ELSE 'member' END)
        END)::org.group_role
FROM assigned a
JOIN org.groups gr ON gr.name = a.group_name
ON CONFLICT (group_id, user_id) DO NOTHING;

-- -----------------------------------------------------------------------------
-- 5. Sample projects (created_by = the owner group's leader)
--    No unique key on projects, so each insert is guarded by NOT EXISTS (name).
-- -----------------------------------------------------------------------------
INSERT INTO project.projects (owner_group_id, created_by_user_id, name, description, status, progress)
SELECT g.id, lead.user_id, p.name, p.descr, p.status::project.project_status, p.progress
FROM (VALUES
    ('Atlas Platform',   'Engineering',      'Core platform revamp',      'active',   65),
    ('Phoenix Mobile',   'Engineering',      'Mobile app rewrite',        'planning', 10),
    ('Helios Analytics', 'Product & Design', 'Analytics dashboard',       'active',   45),
    ('Orion CRM',        'Sales',            'CRM integration',           'on_hold',  30),
    ('Nimbus Campaign',  'Marketing',        'Q3 brand campaign',         'active',   80),
    ('Quantum Ledger',   'Finance & Ops',    'Finance automation',        'planning', 5),
    ('Beacon Support',   'Customer Support', 'Self-service support portal','active',  55),
    ('Vertex Roadmap',   'Executive Office', 'Company-wide OKRs',         'active',   70)
) AS p(name, owner_group, descr, status, progress)
JOIN org.groups g ON g.name = p.owner_group
JOIN LATERAL (
    SELECT m.user_id FROM org.memberships m
    WHERE m.group_id = g.id AND m.role = 'leader' AND m.deactivated_at IS NULL
    LIMIT 1
) AS lead ON true
WHERE NOT EXISTS (SELECT 1 FROM project.projects pp WHERE pp.name = p.name);

-- -----------------------------------------------------------------------------
-- 6. Project collaborators (non-owner groups; the no-self-collab trigger
--    forbids owner == collaborator)
-- -----------------------------------------------------------------------------
INSERT INTO project.project_collaborators (project_id, group_id)
SELECT pr.id, cg.id
FROM (VALUES
    ('Atlas Platform',   'Product & Design'),
    ('Helios Analytics', 'Engineering'),
    ('Orion CRM',        'Marketing')
) AS c(project_name, collab_group)
JOIN project.projects pr ON pr.name = c.project_name
JOIN org.groups cg       ON cg.name = c.collab_group
ON CONFLICT (project_id, group_id) DO NOTHING;

-- -----------------------------------------------------------------------------
-- 7. Sample requests
--    Statuses beyond draft/submitted/cancelled require an assignee
--    (chk_requests_assignee_required_after_submitted) -> use the owner leader.
-- -----------------------------------------------------------------------------
INSERT INTO project.requests (project_id, creator_user_id, assignee_user_id, title, description, status, priority, completed_at)
SELECT pr.id, lead.user_id,
       (CASE WHEN r.status IN ('draft','submitted','cancelled') THEN NULL ELSE lead.user_id END),
       r.title, '', r.status::project.request_status, r.priority::project.request_priority,
       (CASE WHEN r.status = 'completed' THEN NOW() - INTERVAL '3 days' ELSE NULL END)
FROM (VALUES
    ('Atlas Platform',   'Design auth module',        'in_progress', 'high'),
    ('Atlas Platform',   'Set up CI pipeline',        'review',      'normal'),
    ('Atlas Platform',   'Draft API spec',            'draft',       'low'),
    ('Atlas Platform',   'Performance audit',         'assigned',    'urgent'),
    ('Phoenix Mobile',   'Wireframe screens',         'submitted',   'normal'),
    ('Phoenix Mobile',   'Pick cross-platform stack', 'assigned',    'high'),
    ('Helios Analytics', 'Build ETL job',             'in_progress', 'high'),
    ('Helios Analytics', 'Dashboard mockups',         'completed',   'normal'),
    ('Orion CRM',        'Map CRM fields',            'assigned',    'normal'),
    ('Orion CRM',        'Vendor evaluation',         'draft',       'low'),
    ('Nimbus Campaign',  'Creative brief',            'review',      'high'),
    ('Nimbus Campaign',  'Media plan',                'in_progress', 'urgent'),
    ('Quantum Ledger',   'Requirements gathering',    'submitted',   'normal'),
    ('Quantum Ledger',   'Compliance checklist',      'assigned',    'high'),
    ('Beacon Support',   'Knowledge base import',     'in_progress', 'normal'),
    ('Beacon Support',   'SLA definitions',           'completed',   'high'),
    ('Vertex Roadmap',   'Q3 OKR draft',              'review',      'high'),
    ('Vertex Roadmap',   'Budget alignment',          'assigned',    'normal')
) AS r(project_name, title, status, priority)
JOIN project.projects pr ON pr.name = r.project_name
JOIN LATERAL (
    SELECT m.user_id FROM org.memberships m
    WHERE m.group_id = pr.owner_group_id AND m.role = 'leader' AND m.deactivated_at IS NULL
    LIMIT 1
) AS lead ON true
WHERE NOT EXISTS (
    SELECT 1 FROM project.requests rr WHERE rr.project_id = pr.id AND rr.title = r.title
);

-- -----------------------------------------------------------------------------
-- 8. Sample tickets
--    Requester = an active employee (nth by email). Assignee (when the status
--    implies one) = an IT-group member. Timestamps follow the schema CHECKs:
--    priority/triaged_at required once status<>'open'; closed_at iff 'closed'.
-- -----------------------------------------------------------------------------
WITH it_member AS (
    SELECT m.user_id
    FROM org.memberships m
    JOIN org.groups g ON g.id = m.group_id
    WHERE g.kind = 'it' AND m.deactivated_at IS NULL
    ORDER BY m.user_id
    LIMIT 1
),
emp AS (
    SELECT u.id, row_number() OVER (ORDER BY u.email) AS n
    FROM auth.users u
    WHERE u.email LIKE '%@portal.local'
      AND u.status = 'active'
      AND u.email NOT IN ('admin@portal.local', 'hr@portal.local', 'it.lead@portal.local')
),
spec AS (
    SELECT * FROM (VALUES
        ( 1, 'Laptop will not boot',     'hardware', 'open',        NULL),
        ( 2, 'VPN access request',       'access',   'triaged',     'normal'),
        ( 3, 'Email sync broken',        'software', 'assigned',    'high'),
        ( 4, 'Monitor flickering',       'hardware', 'in_progress', 'normal'),
        ( 5, 'Password reset',           'access',   'resolved',    'low'),
        ( 6, 'Printer offline',          'hardware', 'closed',      'normal'),
        ( 7, 'Install IDE license',      'software', 'assigned',    'normal'),
        ( 8, 'Slow office network',      'other',    'open',        NULL),
        ( 9, 'Disk almost full',         'hardware', 'in_progress', 'high'),
        (10, '2FA device lost',          'access',   'resolved',    'urgent'),
        (11, 'Software crash on save',   'software', 'triaged',     'high'),
        (12, 'New hire workstation',     'access',   'closed',      'normal')
    ) AS t(idx, title, category, status, priority)
)
INSERT INTO ticket.tickets
    (requester_user_id, assignee_user_id, title, description, status, priority, category,
     triaged_at, resolved_at, closed_at)
SELECT
    e.id,
    (CASE WHEN s.status IN ('assigned','in_progress','resolved','closed')
          THEN (SELECT user_id FROM it_member) ELSE NULL END),
    s.title, '',
    s.status::ticket.ticket_status,
    s.priority::ticket.ticket_priority,
    s.category::ticket.ticket_category,
    (CASE WHEN s.status <> 'open'                  THEN NOW() - INTERVAL '2 days' ELSE NULL END),
    (CASE WHEN s.status IN ('resolved','closed')   THEN NOW() - INTERVAL '1 day'  ELSE NULL END),
    (CASE WHEN s.status =  'closed'                THEN NOW()                     ELSE NULL END)
FROM spec s
JOIN emp e ON e.n = s.idx
WHERE NOT EXISTS (
    SELECT 1 FROM ticket.tickets tt WHERE tt.title = s.title AND tt.requester_user_id = e.id
);

COMMIT;
