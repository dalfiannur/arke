#!/usr/bin/env bash
# Orkestrator benchmark lintas-bahasa: arke-postgres (Rust) vs BunSane (TS/Bun),
# keduanya EC-store di atas Postgres yang sama. Membangun kedua sisi, menjalankan
# workload identik, lalu menampilkan tabel perbandingan.
#
# Pakai:  ./run.sh [N] [ITERS] [C]
# Env:    PGHOST/PGPORT/PGUSER/PGPASS (default postgres:postgres@localhost:5432)
#
# `C` = konkurensi tulis, disetel SAMA di kedua sisi (arke `--concurrency C`,
# BunSane `SAVE_CONCURRENCY=C`) → perbandingan apel-ke-apel. Default 8. TANPA ini,
# arke default C=1 (sekuensial) vs BunSane C=20 → `save` timpang & menyesatkan.
# Untuk kurva scaling lintas beberapa C, pakai `sweep.sh`.
set -euo pipefail
cd "$(dirname "$0")"

N="${1:-20000}"
ITERS="${2:-5}"
C="${3:-8}"
PGHOST="${PGHOST:-localhost}"
PGPORT="${PGPORT:-5432}"
PGUSER="${PGUSER:-postgres}"
PGPASS="${PGPASS:-postgres}"

ARKE_DB="postgres://${PGUSER}:${PGPASS}@${PGHOST}:${PGPORT}/arke_bench"
BUNSANE_DB="postgres://${PGUSER}:${PGPASS}@${PGHOST}:${PGPORT}/bunsane_bench"

echo "==> N=${N} ITERS=${ITERS} C=${C} (konkurensi tulis disamakan kedua sisi)"

echo "==> build arke (release)"
( cd arke && cargo build --release --quiet )

echo "==> install bunsane deps"
( cd bunsane && bun install --silent >/dev/null 2>&1 || bun install )

echo "==> run arke-postgres (--concurrency ${C})"
DATABASE_URL="$ARKE_DB" \
  arke/target/release/arke-pg-bench --n "$N" --iters "$ITERS" --concurrency "$C" --json > /tmp/arke_pg.json
cat /tmp/arke_pg.json | grep -E 'workload|ms_avg' >/dev/null # sanity

echo "==> run bunsane (SAVE_CONCURRENCY=${C})"
( cd bunsane && DB_CONNECTION_URL="$BUNSANE_DB" \
    SAVE_CONCURRENCY="$C" \
    BUNSANE_DEFAULT_QUERY_LIMIT=100000000 LOG_LEVEL=silent \
    bun bench.ts --n "$N" --iters "$ITERS" --json ) > /tmp/bunsane_pg.json

echo "==> compare"
bun compare.ts /tmp/arke_pg.json /tmp/bunsane_pg.json
