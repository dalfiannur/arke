#!/usr/bin/env bash
# Sweep konkurensi: jalankan KEDUA engine pada beberapa level konkurensi tulis
# (default 1 4 8 16) → kurva scaling multi-core. Baca (load/filter) ~datar.
#
# Pakai:  ./sweep.sh [N] [ITERS] [LEVELS...]
#   ./sweep.sh                 # N=20000 iters=5, levels "1 4 8 16"
#   ./sweep.sh 10000 5 1 4 8 16 12
set -euo pipefail
cd "$(dirname "$0")"

N="${1:-20000}"; shift || true
ITERS="${1:-5}"; shift || true
LEVELS=("$@"); [ ${#LEVELS[@]} -eq 0 ] && LEVELS=(1 4 8 16)

PGHOST="${PGHOST:-localhost}"; PGPORT="${PGPORT:-5432}"
PGUSER="${PGUSER:-postgres}"; PGPASS="${PGPASS:-postgres}"
ARKE_DB="postgres://${PGUSER}:${PGPASS}@${PGHOST}:${PGPORT}/arke_bench"
BUNSANE_DB="postgres://${PGUSER}:${PGPASS}@${PGHOST}:${PGPORT}/bunsane_bench"

OUT="$(mktemp -d)"
echo "==> N=${N} ITERS=${ITERS} LEVELS=${LEVELS[*]}  (out: $OUT)"

echo "==> build arke (release)"; ( cd arke && cargo build --release --quiet )
echo "==> bunsane deps";        ( cd bunsane && bun install --silent >/dev/null 2>&1 || true )

FILES=()
for C in "${LEVELS[@]}"; do
  echo "==> concurrency=$C : arke"
  DATABASE_URL="$ARKE_DB" arke/target/release/arke-pg-bench \
    --n "$N" --iters "$ITERS" --concurrency "$C" --json > "$OUT/arke_$C.json"
  FILES+=("$OUT/arke_$C.json")

  echo "==> concurrency=$C : bunsane"
  ( cd bunsane && DB_CONNECTION_URL="$BUNSANE_DB" \
      BUNSANE_DEFAULT_QUERY_LIMIT=100000000 LOG_LEVEL=silent SAVE_CONCURRENCY="$C" \
      bun bench.ts --n "$N" --iters "$ITERS" --json ) > "$OUT/bunsane_$C.json"
  FILES+=("$OUT/bunsane_$C.json")
done

echo "==> report"
bun sweep_report.ts "${FILES[@]}"
