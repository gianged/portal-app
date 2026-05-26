#!/bin/bash
# Runs once on first Postgres start (only when the data dir is empty).
# Creates the `openfga` database alongside the main `portal` database so a
# single Postgres instance can serve both the application AND OpenFGA's
# datastore. OpenFGA's own `migrate` command then creates its schema inside
# the `openfga` database when the openfga-migrate service runs.
set -euo pipefail

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    CREATE DATABASE openfga;
    GRANT ALL PRIVILEGES ON DATABASE openfga TO "$POSTGRES_USER";
EOSQL

echo "[init-multidb] created database: openfga"
