#!/bin/sh
# One-time, idempotent bootstrap for the dependency stack. Run via `cargo make bootstrap`.
#
# Brings the stores up, migrates OpenFGA's datastore, applies the Scylla schema,
# then starts OpenFGA. The server resolves (get-or-creates) the "portal" store
# and uploads the authorization model on first boot, so there is no store-init
# step here. Every step is idempotent, so this is safe to re-run; a fresh
# `down -v` triggers a clean re-bootstrap.
set -eu

# When launched via a bare sh.exe (cargo-make on Windows points at Git's sh),
# MSYS /usr/bin isn't on PATH, so coreutils (dirname, cat, awk...) aren't found.
# Prepend the standard bin dirs -- harmless on Linux where they're already std.
export PATH="/usr/bin:/bin:$PATH"

cd "$(dirname "$0")/../.."        # repo root
INFRA="docker compose --env-file .env -f infra/docker-compose.infra.yml"

# Stop Git Bash (MSYS) from rewriting container-side paths like /config when
# passed to docker.exe. No-op on Linux.
export MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*'

# 1. Stores up + wait for health. Postgres' first boot runs the
#    docker-entrypoint-initdb.d scripts -> creates the openfga db + app schema.
$INFRA up -d --wait postgres scylla redis

# 2. OpenFGA datastore migration (idempotent; must precede `openfga run`).
$INFRA run --rm --no-deps openfga migrate

# 3. Scylla schema (schema.cql is CREATE ... IF NOT EXISTS throughout).
$INFRA exec -T scylla cqlsh < infra/scylla/schema.cql

# 4. OpenFGA daemon. The server resolves (get-or-creates) the "portal" store and
#    uploads the authorization model on first boot, so there is no store-init step.
$INFRA up -d openfga

echo "[init] done. Stores up, OpenFGA migrated and running; the server initialises the OpenFGA store on first boot."
