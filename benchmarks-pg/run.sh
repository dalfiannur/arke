#!/usr/bin/env bash
# Orkestrator benchmark lintas-bahasa: arke-postgres (Rust) vs BunSane (TS/Bun),
# keduanya EC-store di atas Postgres yang sama. Membangun kedua sisi, menjalankan
# workload identik, lalu menampilkan tabel perbandingan.
#
# Pakai:  ./run.sh [N] [ITERS]
# Env:    PGHOST/PGPORT/PGUSER/PGPASS (default postgres:postgres@localhost:5432)
set -euo pipefail
cd "$(dirname "$0")"

N="${1:-20000}"
ITERS="${2:-5}"
PGHOST="${PGHOST:-localhost}"
PGPORT="${PGPORT:-5432}"
PGUSER="${PGUSER:-postgres}"
PGPASS="${PGPASS:-postgres}"

ARKE_DB="postgres://${PGUSER}:${PGPASS}@${PGHOST}:${PGPORT}/arke_bench"
BUNSANE_DB="postgres://${PGUSER}:${PGPASS}@${PGHOST}:${PGPORT}/bunsane_bench"

echo "==> N=${N} ITERS=${ITERS}"

echo "==> build arke (release)"
( cd arke && cargo build --release --quiet )

echo "==> install bunsane deps"
( cd bunsane && bun install --silent >/dev/null 2>&1 || bun install )

echo "==> run arke-postgres"
DATABASE_URL="$ARKE_DB" \
  arke/target/release/arke-pg-bench --n "$N" --iters "$ITERS" --json > /tmp/arke_pg.json
cat /tmp/arke_pg.json | grep -E 'workload|ms_avg' >/dev/null # sanity

echo "==> run bunsane"
( cd bunsane && DB_CONNECTION_URL="$BUNSANE_DB" \
    BUNSANE_DEFAULT_QUERY_LIMIT=100000000 LOG_LEVEL=silent \
    bun bench.ts --n "$N" --iters "$ITERS" --json ) > /tmp/bunsane_pg.json

echo "==> compare"
bun compare.ts /tmp/arke_pg.json /tmp/bunsane_pg.json
