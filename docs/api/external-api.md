# Portal External API Contract — `/api/ext/v1`

Read-only HTTP API for scripts and reporting tools that need to pull project,
work-request, and company-report data out of the portal. It is deliberately
minimal: JSON over HTTP, key-based auth, keyset pagination, no mutations.

- Base URL (dev): `http://127.0.0.1:8090/api/ext/v1`
- Base URL (prod): `http://<portal-host>:<SERVER_PORT>/api/ext/v1`
- All endpoints are `GET`. Any write attempt is a `404`/`405` by design.
- The API is versioned by path. `v1` responses may gain new fields over time;
  clients MUST ignore unknown fields. Fields are never removed or renamed
  within `v1`.

---

## 1. Prerequisites

1. **Network**: the portal only answers callers inside the allowlisted
   networks (`IP_ALLOWLIST`, defaults to loopback + RFC1918/ULA private
   ranges). Run your script from the office LAN / VPN. Blocked callers get
   `403` before authentication.
2. **API key**: a *service account* key issued by a Director or HR (see next
   section). Keys look like `pak_<64 hex chars>`.

## 2. Getting a key (admins)

Service accounts are managed by Director/HR through the regular portal API
(session-cookie auth, NOT the ext API):

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/api/v1/service-accounts` | Create an account, returns the key **once** |
| `GET` | `/api/v1/service-accounts` | List accounts (never shows keys) |
| `DELETE` | `/api/v1/service-accounts/{id}` | Revoke — the key stops working immediately |

Create request body:

```json
{
  "name": "monthly-report-script",
  "scopes": ["projects", "requests", "reports"]
}
```

- `name`: unique, human-readable owner of the key.
- `scopes`: at least one of `"projects"`, `"requests"`, `"reports"`. Each
  scope unlocks the matching endpoint group below; calls outside the granted
  scopes return `403 forbidden`.

Create response (the ONLY time the key is visible — store it in a secret
manager, it cannot be retrieved again):

```json
{
  "account": {
    "id": "019f427b-b979-7952-a9b3-768b27e357f2",
    "name": "monthly-report-script",
    "status": "active",
    "created_by": "afe570c1-afef-4bab-a609-1d08eea98658",
    "revoked_at": null,
    "created_at": "2026-07-08T16:07:12.505Z"
  },
  "key": "pak_1c35bc1c0f95b186a92484f4332ff25349fd784287d478a3f54e06ed6338dc64"
}
```

If a key leaks: revoke it (`DELETE`) and create a new account. Revocation is
instant; only the key's SHA-256 hash is stored server-side.

## 3. Authentication

Send the key as a bearer token on every request:

```
Authorization: Bearer pak_1c35bc1c0f95b186a92484f4332ff25349fd784287d478a3f54e06ed6338dc64
```

| Situation | Response |
| --- | --- |
| Header missing / malformed | `401 unauthenticated` |
| Unknown or revoked key | `401 unauthenticated` |
| Valid key, missing scope for the endpoint | `403 forbidden` |

## 4. Rate limiting

Each key gets `EXT_RATE_LIMIT` requests per rolling window
(`RATE_LIMIT_WINDOW_SECS`; defaults: **60 requests / 60 seconds**). Exceeding
it returns `429 rate_limited`. Back off and retry after the window; for bulk
exports prefer larger `limit` values over more requests.

## 5. Conventions

- Responses are JSON, UTF-8.
- IDs are UUID strings (UUIDv7 — lexically ordered by creation time).
- Timestamps are RFC 3339 / ISO 8601 in UTC, e.g. `"2026-07-08T16:08:46.512Z"`.
  Nullable timestamps are `null` when absent.
- Enums are lower `snake_case` string tokens (tables below). Treat unknown
  tokens as forward-compatible additions.
- Every non-2xx response carries a stable error envelope:

```json
{ "code": "forbidden", "message": "forbidden" }
```

| HTTP | `code` | Meaning |
| --- | --- | --- |
| 400 | `validation` | Bad parameter (malformed uuid, month out of range, …) |
| 401 | `unauthenticated` | Missing/unknown/revoked key |
| 403 | `forbidden` | Key lacks the required scope (or IP not allowlisted) |
| 404 | `not_found` | Entity does not exist |
| 429 | `rate_limited` | Per-key ceiling exceeded |
| 500 | `internal` | Server fault — retry later, report if persistent |

## 6. Pagination

List endpoints use keyset pagination:

- `limit` — page size. Default `100`, maximum `500` (larger values are
  clamped, `0` means default).
- `after` — exclusive cursor: the `next_cursor` value from the previous page
  (a UUID). Omit for the first page.

Responses wrap items in a page envelope:

```json
{ "items": [ ... ], "next_cursor": "019f427d-28b0-76a0-9ed0-9ca5125264c5" }
```

Contract:

- `next_cursor` is non-null only when the page came back full, i.e. more rows
  MAY remain. Loop until `next_cursor` is `null`.
- Ordering is ascending by `id` (UUIDv7 ≈ creation order) and stable across
  pages; rows created mid-scan may or may not appear.
- There is no total-count field; count by iterating.

Reference loop (Python):

```python
import requests

BASE = "http://127.0.0.1:8090/api/ext/v1"
HEADERS = {"Authorization": "Bearer pak_..."}

def fetch_all(path, **params):
    cursor = None
    while True:
        q = {**params, "limit": 500, **({"after": cursor} if cursor else {})}
        page = requests.get(f"{BASE}{path}", headers=HEADERS, params=q, timeout=30)
        page.raise_for_status()
        body = page.json()
        yield from body["items"]
        cursor = body["next_cursor"]
        if cursor is None:
            break

projects = list(fetch_all("/projects"))
```

## 7. Endpoints

| Method | Path | Scope | Returns |
| --- | --- | --- | --- |
| `GET` | `/projects` | `projects` | Page of project records |
| `GET` | `/projects/{id}` | `projects` | One project record |
| `GET` | `/requests` | `requests` | Page of request records |
| `GET` | `/requests/{id}` | `requests` | One request record |
| `GET` | `/reports/monthly` | `reports` | Company monthly aggregates |
| `GET` | `/reports/yearly` | `reports` | Company yearly growth series |

### 7.1 `GET /projects`

Query: `after` (uuid, optional), `limit` (int, optional).

Project record:

```json
{
  "id": "019f4292-...",
  "owner_group_id": "019f4288-...",
  "created_by_user_id": "afe570c1-...",
  "name": "Helios",
  "description": "Migration of the billing pipeline",
  "status": "active",
  "progress": 40,
  "completed_at": null,
  "created_at": "2026-05-01T08:00:00Z",
  "updated_at": "2026-07-01T09:30:00Z"
}
```

| Field | Type | Notes |
| --- | --- | --- |
| `id` | uuid | |
| `owner_group_id` | uuid | Exactly one owner group per project |
| `created_by_user_id` | uuid | |
| `name`, `description` | string | `description` may be empty |
| `status` | enum | `planning` \| `active` \| `on_hold` \| `completed` \| `cancelled` |
| `progress` | int 0–100 | Manual completion %, set by group leaders |
| `completed_at` | timestamp\|null | Set when `status` = `completed` |
| `created_at`, `updated_at` | timestamp | |

### 7.2 `GET /projects/{id}`

Path: `id` (uuid). Returns one project record or `404`.

### 7.3 `GET /requests`

Query: `project` (uuid, optional — filter to one project), `after`, `limit`.

Request record:

```json
{
  "id": "019f42a0-...",
  "project_id": "019f4292-...",
  "creator_user_id": "afe570c1-...",
  "assignee_user_id": null,
  "title": "Provision staging database",
  "description": "",
  "status": "submitted",
  "priority": "normal",
  "progress": 0,
  "due_at": "2026-07-20T00:00:00Z",
  "completed_at": null,
  "created_at": "2026-07-08T10:00:00Z",
  "updated_at": "2026-07-08T10:00:00Z"
}
```

| Field | Type | Notes |
| --- | --- | --- |
| `status` | enum | `draft` \| `submitted` \| `assigned` \| `in_progress` \| `review` \| `completed` \| `cancelled` |
| `priority` | enum | `low` \| `normal` \| `high` \| `urgent` |
| `assignee_user_id` | uuid\|null | Null until assigned |
| `due_at`, `completed_at` | timestamp\|null | |
| others | | Same conventions as the project record |

### 7.4 `GET /requests/{id}`

Path: `id` (uuid). Returns one request record or `404`.

### 7.5 `GET /reports/monthly?year=2026&month=7`

Query: `year` (int, required), `month` (1–12, required). Aggregates for that
calendar month:

```json
{
  "year": 2026,
  "month": 7,
  "groups": [
    {
      "group_id": "019f4288-...",
      "group_name": "Platform",
      "is_it": false,
      "projects_total": 5, "projects_completed": 1, "projects_active": 3,
      "projects_on_hold": 0, "projects_stuck": 1, "avg_project_progress": 62,
      "requests_total": 40, "requests_completed": 31, "requests_open": 9,
      "request_completion_pct": 78,
      "headcount": 12
    }
  ],
  "tickets": {
    "created_in_period": 25,
    "resolved_in_period": 22,
    "avg_resolve_hours": 6.4,
    "by_status":   [ { "label": "resolved", "count": 22 } ],
    "by_category": [ { "label": "hardware", "count": 9 } ]
  },
  "staff": {
    "company_headcount": 240,
    "new_joiners": 4,
    "deactivations": 1,
    "per_group": [ { "group_id": "…", "group_name": "Platform", "headcount": 12 } ]
  }
}
```

### 7.6 `GET /reports/yearly?year=2026`

Query: `year` (int, required). Twelve monthly points per series (oldest
first) plus headline totals:

```json
{
  "year": 2026,
  "growth": {
    "headcount":          [ { "year": 2026, "month": 1, "value": 3 } ],
    "new_joiners":        [ { "year": 2026, "month": 1, "value": 4 } ],
    "tickets_created":    [ { "year": 2026, "month": 1, "value": 31 } ],
    "projects_completed": [ { "year": 2026, "month": 1, "value": 2 } ],
    "requests_completed": [ { "year": 2026, "month": 1, "value": 55 } ]
  },
  "totals": {
    "company_headcount": 240,
    "net_headcount_change": 18,
    "new_hires": 25,
    "departures": 7,
    "tickets_created": 300,
    "projects_completed": 21,
    "requests_completed": 610
  }
}
```

`growth.headcount.value` is the cumulative net headcount change within the
year; the other series are per-month counts.

## 8. Quick start (curl)

```bash
KEY="pak_..."
BASE="http://127.0.0.1:8090/api/ext/v1"

# First page of projects
curl -s -H "Authorization: Bearer $KEY" "$BASE/projects?limit=100"

# Requests of one project
curl -s -H "Authorization: Bearer $KEY" "$BASE/requests?project=019f4292-...&limit=100"

# July 2026 company report
curl -s -H "Authorization: Bearer $KEY" "$BASE/reports/monthly?year=2026&month=7"
```

---

## Appendix: internal gRPC plane (not for external scripts)

Server-to-server traffic uses a separate gRPC plane; external scripts must
use the REST contract above instead.

- Contract source of truth: `crates/proto/proto/portal/internal/v1/`
  (`jobs.proto` — job ingest on the workers, port `50052`; `query.proto` —
  read plane on the server, port `50051`).
- Auth: static bearer token in the `authorization` metadata
  (`Bearer <INTERNAL_GRPC_TOKEN>`); both binaries share the value from `.env`.
- The workers also serve the standard `grpc.health.v1` service,
  unauthenticated, for liveness probes.
