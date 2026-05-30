#!/bin/sh
# On-demand backup of Postgres + ScyllaDB. Run via `cargo make backup`.
#
# Dumps every Postgres database (portal + openfga) plus roles, and snapshots the
# chat keyspace, into BACKUP_DIR (default ./backups) on the host, then prunes
# archives older than BACKUP_KEEP_DAYS. Assumes the stack is up (cargo make bootstrap).
#
# To run on a schedule, point cron / Task Scheduler at this script -- it does not
# install any always-on container itself.
set -eu

# When launched via a bare sh.exe (cargo-make on Windows points at Git's sh),
# MSYS /usr/bin isn't on PATH, so coreutils (dirname, find, date...) aren't found.
# Prepend the standard bin dirs -- harmless on Linux where they're already std.
export PATH="/usr/bin:/bin:$PATH"

cd "$(dirname "$0")/../.."        # repo root
INFRA="docker compose --env-file .env -f infra/docker-compose.infra.yml"

BACKUP_DIR="${BACKUP_DIR:-./backups}"
KEEP_DAYS="${BACKUP_KEEP_DAYS:-7}"
PG_USER="${POSTGRES_USER:-portal}"
PG_PASS="${POSTGRES_PASSWORD:-portal}"
KEYSPACE="${SCYLLA_KEYSPACE:-portal_chat}"
TS="$(date +%Y%m%d-%H%M%S)"

# Resolve the running containers by compose service (same project regardless of
# which compose file started them, thanks to the shared `name: portal`).
PG=$($INFRA ps -q postgres)
SC=$($INFRA ps -q scylla)
if [ -z "$PG" ] || [ -z "$SC" ]; then
    echo "[backup] postgres/scylla not running -- start the stack first (cargo make bootstrap)." >&2
    exit 1
fi

mkdir -p "$BACKUP_DIR"

# Postgres: all databases + roles in one gzipped logical dump.
echo "[backup] postgres -> $BACKUP_DIR/postgres-$TS.sql.gz"
docker exec -e PGPASSWORD="$PG_PASS" "$PG" \
    pg_dumpall -U "$PG_USER" -h 127.0.0.1 | gzip > "$BACKUP_DIR/postgres-$TS.sql.gz"

# Scylla: snapshot the keyspace, stream the files out, then drop the snapshot.
TAG="portal-backup-$TS"
echo "[backup] scylla snapshot '$TAG' of keyspace '$KEYSPACE'"
docker exec "$SC" nodetool snapshot -t "$TAG" "$KEYSPACE" >/dev/null
docker exec "$SC" sh -c "cd /var/lib/scylla/data && tar czf - */*/snapshots/$TAG 2>/dev/null" \
    > "$BACKUP_DIR/scylla-$TS.tar.gz"
docker exec "$SC" nodetool clearsnapshot -t "$TAG" "$KEYSPACE" >/dev/null 2>&1 || true

# Prune old archives.
echo "[backup] pruning archives older than $KEEP_DAYS days in $BACKUP_DIR"
find "$BACKUP_DIR" -maxdepth 1 -type f \( -name 'postgres-*.sql.gz' -o -name 'scylla-*.tar.gz' \) \
    -mtime "+$KEEP_DAYS" -print -delete

echo "[backup] done -> $BACKUP_DIR"
