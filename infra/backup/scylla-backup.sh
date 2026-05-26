#!/bin/sh
# Triggered by cron inside the scylla-backup sidecar. Uses docker-cli + the
# host docker socket to `nodetool snapshot` the live scylla container, tars
# the resulting snapshot dir into the named backup volume, then clears the
# in-container snapshot to avoid leaking hardlinks. Prunes backups older
# than BACKUP_KEEP_DAYS.
#
# Requires:
#   /var/run/docker.sock mounted (read-only)
#   /backups volume mounted (read-write)
#   env: KEYSPACE, BACKUP_KEEP_DAYS
set -eu

KEYSPACE="${KEYSPACE:-portal_chat}"
KEEP_DAYS="${BACKUP_KEEP_DAYS:-7}"
TS="$(date +%Y%m%d-%H%M%S)"
TAG="portal-backup-$TS"
OUT="/backups/scylla-$TS.tar.gz"

echo "[scylla-backup] $TS: taking snapshot '$TAG' of keyspace '$KEYSPACE'"
docker exec scylla nodetool snapshot -t "$TAG" "$KEYSPACE"

echo "[scylla-backup] streaming snapshot files to $OUT"
docker exec scylla sh -c "cd /var/lib/scylla/data && tar czf - */*/snapshots/$TAG 2>/dev/null" > "$OUT"

echo "[scylla-backup] clearing snapshot inside scylla container"
docker exec scylla nodetool clearsnapshot -t "$TAG" "$KEYSPACE" || true

echo "[scylla-backup] pruning backups older than $KEEP_DAYS days"
find /backups -maxdepth 1 -name 'scylla-*.tar.gz' -type f -mtime "+$KEEP_DAYS" -print -delete

echo "[scylla-backup] done: $OUT ($(du -h "$OUT" | cut -f1))"
