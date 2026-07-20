# Load tests

Go harness modelling 1000 employees at peak. Run against the host-dev stack
(`cargo make up` + server/workers running) — no external load-test tool needed.

## Prerequisites

1. A seeded database: `cargo make bootstrap && cargo make seed`
   (~100 active `@portal.local` accounts, shared password `admin123`).
2. The server started with `COOKIE_SECURE=false` (plain-HTTP session cookies).
3. `users.json` in this directory — the active seeded emails:

   ```powershell
   docker exec portal-postgres-1 psql -U portal -d portal -Atc `
     "SELECT email FROM auth.users WHERE status='active' AND email LIKE '%@portal.local'" |
     ConvertTo-Json | Out-File -Encoding ascii loadtest/users.json
   ```

## Scenarios

Run from this directory (`cd loadtest`):

| Command | Models | Default shape |
| --- | --- | --- |
| `go run . login-storm` | 09:00 login rush | 200 logins/min for 5m |
| `go run . api-mix` | steady REST traffic | ramp 50 -> 200 rps, hold 3m (`-peak-rps 500` for stress) |
| `go run . ws-chat` | live chat fan-out | 200 sockets on general, HR sender every 6s (`-sockets 1000` for full scale) |

Common flags: `-base-url` (default `http://127.0.0.1:8090`), `-users`, `-password`.
Each scenario prints latency percentiles plus its threshold checks and exits
non-zero when one fails.

## Boot-resilience e2e

`go run . boot-resilience` (or `cargo make boot-resilience-test` from the repo
root) verifies that server and workers wait for infra at startup instead of
failing fast:

1. Brings the dependency stack up, then stops `postgres` + `scylla`.
2. Starts the prebuilt binaries and asserts both stay alive and log retry
   warnings for the outage window (default 15s, `-outage`).
3. Restarts the two services and asserts `/healthz` + `/readyz` go 200 and the
   workers gRPC port accepts within 3 minutes.
4. Reruns the server with `STARTUP_TIMEOUT_SECS=8` while Postgres is stopped
   and asserts a non-zero exit once the budget expires.

Prerequisites: `cargo build -p server -p workers`, docker running, and ports
8090/50052 free (the scenario aborts if a live server/workers already answers).
Child output lands in `boot-*.log` here. No seeded data or users.json needed.

## Concurrency race e2e

`go run . race` (or `cargo make race-test` from the repo root) checks that
concurrent writers cannot corrupt an entity. It provisions its own throwaway
cast (leader, sub-leaders, members, IT staff) via `hr@portal.local`, then per
scenario fires barrier-synchronized bursts at the same entity:

- request: submit x N, assign to different users, approve vs reject, and
  concurrent single-field PATCHes
- ticket: triage x N (distinct priorities), assign to different IT users
- project: hold/complete/cancel race, resume x N from on-hold
- group: two simultaneous leader promotions (one-leader invariant)
- user: deactivate x N

Contract per burst: exactly one winner (all winners for the compatible PATCH
burst), losers get 409 (never 500), and the final state read back must match
the winner. Needs the demo seed (`hr@portal.local`) and `COOKIE_SECURE=false`;
no `users.json`. Defaults (`-writers 6 -rounds 8`) stay under the per-user API
rate limit — raise `API_RATE_LIMIT` before raising them.

## Rate-limit interplay

- The defaults (`API_RATE_LIMIT=120`/min/user) throttle `api-mix` above ~2 req/s
  per user. With only ~100 seeded accounts, `-peak-rps 500` **will** produce
  429s; either raise `API_RATE_LIMIT` on the server for the stress run or read
  the 429 count as the limiter doing its job (429 is counted separately, not as
  a failure).
- `ws-chat` senders share the single HR account (posting to general is HR-only),
  so keep senders x (60s / `-send-every`) under `CHAT_RATE_LIMIT` (120/min).
- Sockets/logins are staggered (`-stagger`) so the connect wave stays under
  `AUTH_IP_RATE_LIMIT` (600/min from one IP); an unstaggered stampede trips it
  by design.

## Pass criteria

Thresholds are encoded per scenario: p95 latency (1s login storm, 500ms API),
<1% transport/5xx failures, zero-ish WS errors, and at least one fan-out frame
received.
