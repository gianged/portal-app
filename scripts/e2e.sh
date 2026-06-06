#!/usr/bin/env bash
# Orchestrates a full local end-to-end run: dependency stack + server + workers +
# Trunk-served frontend + geckodriver, then the fantoccini smoke test. Best-effort
# cleanup of spawned processes on exit.
set -euo pipefail
cd "$(dirname "$0")/.."

pids=()
cleanup() { for p in "${pids[@]:-}"; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT

# 1. Dependency stack (Postgres/Redis/Scylla/OpenFGA) + schema bootstrap.
cargo make bootstrap

# 2. App processes.
cargo run --bin server &  pids+=($!)
cargo run --bin workers & pids+=($!)

# 3. Frontend: Trunk dev server on :8081, proxies /api -> server.
( cd crates/frontend && trunk serve ) & pids+=($!)

# 4. WebDriver.
geckodriver --port 4444 & pids+=($!)

# 5. Let everything settle, then drive the browser.
sleep 8
cargo test -p portal-e2e -- --ignored --nocapture
