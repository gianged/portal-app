#!/bin/sh
# Optional demo seed (dev/testing). Applies infra/seed/demo-seed.sql to the
# running Postgres, then materialises the matching OpenFGA tuples via the
# `seed_authz` binary. Run via `cargo make seed`.
#
# Re-runnable: the SQL is guarded (ON CONFLICT / NOT EXISTS); the authz step
# logs per-tuple warnings for tuples that already exist. For a guaranteed-clean
# run, reset first: `docker compose ... down -v && cargo make bootstrap`.
set -eu

# cargo-make on Windows launches Git's sh.exe, whose MSYS /usr/bin is off PATH.
export PATH="/usr/bin:/bin:$PATH"

cd "$(dirname "$0")/../.."        # repo root
INFRA="docker compose --env-file .env -f infra/docker-compose.infra.yml"

# Stop Git Bash (MSYS) from rewriting container-side paths. No-op on Linux.
export MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*'

# 1. Postgres rows. Piped over stdin so no container-side path is involved.
echo "[seed] applying infra/seed/demo-seed.sql ..."
$INFRA exec -T postgres psql -U portal -d portal -v ON_ERROR_STOP=1 < infra/seed/demo-seed.sql

# 2. OpenFGA tuples derived from the seeded org graph (Postgres + OpenFGA only).
echo "[seed] materialising OpenFGA tuples ..."
cargo run --bin seed_authz

echo "[seed] done."
