#!/bin/sh
# One-shot OpenFGA bootstrap. Idempotent: finds an existing store named
# "portal" or creates one, uploads the authorization model, and writes the
# resulting store ID to /config/openfga-store-id for the server to read.
#
# Run manually after the stack first comes up:
#   docker compose --profile init up openfga-init
#
# Retire this service once server::main implements GetOrCreateStore at startup.
set -eu

API="http://openfga:8080"
STORE_FILE="/config/openfga-store-id"
MODEL_FILE="/openfga/authorization-model.json"

mkdir -p "$(dirname "$STORE_FILE")"

echo "[openfga-init] looking for existing store named 'portal'..."
LIST_JSON=$(curl -fsS "$API/stores")
EXISTING=$(printf '%s' "$LIST_JSON" \
    | tr ',' '\n' \
    | awk -F'"' '/"name":"portal"/{found=1} /"id":/ && !id {id=$4} END{ if(found) print id }')

if [ -n "${EXISTING:-}" ]; then
    echo "[openfga-init] store 'portal' already exists: $EXISTING"
    printf '%s' "$EXISTING" > "$STORE_FILE"
    exit 0
fi

echo "[openfga-init] creating store 'portal'..."
CREATE_JSON=$(curl -fsS -X POST "$API/stores" \
    -H 'content-type: application/json' \
    -d '{"name":"portal"}')

STORE_ID=$(printf '%s' "$CREATE_JSON" \
    | tr ',' '\n' \
    | awk -F'"' '/"id":/{print $4; exit}')

if [ -z "$STORE_ID" ]; then
    echo "[openfga-init] FATAL: could not parse store id from response:" >&2
    echo "$CREATE_JSON" >&2
    exit 1
fi

echo "[openfga-init] uploading authorization model to store $STORE_ID..."
curl -fsS -X POST "$API/stores/$STORE_ID/authorization-models" \
    -H 'content-type: application/json' \
    -d @"$MODEL_FILE" > /dev/null

printf '%s' "$STORE_ID" > "$STORE_FILE"
echo "[openfga-init] done. store id written to $STORE_FILE: $STORE_ID"
