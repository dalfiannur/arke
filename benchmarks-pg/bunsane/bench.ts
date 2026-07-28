// Benchmark BunSane (storage layer, TANPA GraphQL/HTTP) untuk perbandingan
// lintas-bahasa vs arke-postgres. Empat beban kerja atas N entity ber-(Position,
// Health) di Postgres yang sama. Output JSON (--json) → dibandingkan compare.ts.
//
// DB dikonfigurasi via env SEBELUM proses jalan (hindari hoisting ESM):
//   DB_CONNECTION_URL=postgres://postgres:postgres@localhost:5432/bunsane_bench \
//     bun bench.ts --n 20000 --iters 5 [--json]
import "reflect-metadata";
import { Component, CompData, BaseComponent, ComponentRegistry } from "bunsane/core/components";
import { Entity } from "bunsane/core/Entity";
import { Query } from "bunsane/query";
import { PrepareDatabase, HasValidBaseTable } from "bunsane/database/DatabaseHelper";
import ApplicationLifecycle, { ApplicationPhase } from "bunsane/core/ApplicationLifecycle";
import db from "bunsane/database";

@Component
class Position extends BaseComponent {
  @CompData() x: number = 0;
  @CompData() y: number = 0;
}

@Component
class Health extends BaseComponent {
  @CompData({ indexed: true }) hp: number = 0;
}

// LCG deterministik (samakan sebaran hp dgn sisi Rust; angka absolut tak wajib sama).
class Lcg {
  private s: bigint;
  constructor(seed: bigint) {
    this.s = seed ^ 0x9e3779b97f4a7c15n;
  }
  next(): number {
    this.s = (this.s * 6364136223846793005n + 1442695040888963407n) & 0xffffffffffffffffn;
    return Number(this.s >> 33n);
  }
}

// ---- args ----
let N = 20000, ITERS = 5, JSON_OUT = false;
const argv = process.argv.slice(2);
for (let i = 0; i < argv.length; i++) {
  if (argv[i] === "--n") N = parseInt(argv[++i], 10);
  else if (argv[i] === "--iters") ITERS = parseInt(argv[++i], 10);
  else if (argv[i] === "--json") JSON_OUT = true;
}

type Stat = { workload: string; n: number; ms_avg: number; ms_min: number; per_sec: number };

// Ukur async `fn` selama ITERS iterasi (1 warm-up dibuang) → Stat.
async function bench(label: string, count: number, fn: () => Promise<void>): Promise<Stat> {
  await fn(); // warm-up
  const samples: number[] = [];
  for (let i = 0; i < ITERS; i++) {
    const t = performance.now();
    await fn();
    samples.push(performance.now() - t);
  }
  const ms_avg = samples.reduce((a, b) => a + b, 0) / samples.length;
  const ms_min = Math.min(...samples);
  return { workload: label, n: count, ms_avg, ms_min, per_sec: count / (ms_avg / 1000) };
}

// Bangun N entity in-memory (belum di-save).
function buildEntities(): Entity[] {
  const rng = new Lcg(0xa42e5eedn);
  const out: Entity[] = new Array(N);
  for (let i = 0; i < N; i++) {
    out[i] = Entity.Create()
      .add(Position, { x: i, y: N - i })
      .add(Health, { hp: rng.next() % 100 });
  }
  return out;
}

// Simpan banyak entity dgn konkurensi terbatas (memakai pool koneksi). Default
// 20 = jalur cepat idiomatis BunSane; set SAVE_CONCURRENCY=1 utk sekuensial
// (apel-ke-apel dgn arke yg 1-transaksi-sekuensial).
const SAVE_CONCURRENCY = parseInt(process.env.SAVE_CONCURRENCY ?? "20", 10);
async function saveAll(entities: Entity[], concurrency = SAVE_CONCURRENCY): Promise<void> {
  for (let i = 0; i < entities.length; i += concurrency) {
    await Promise.all(entities.slice(i, i + concurrency).map((e) => e.save()));
  }
}

async function truncate(): Promise<void> {
  // Skema BunSane: `entities` + `components` (partitioned by type_id, JSONB).
  // TRUNCATE parent partitioned → semua partisi ikut bersih. RESTART IDENTITY
  // tak relevan (UUID), CASCADE utk FK components→entities.
  await db`TRUNCATE TABLE components, entities CASCADE`;
}

async function main() {
  // Bootstrap storage TANPA HTTP: base table (entities/components) → tandai
  // DATABASE_READY (butuh utk save/delete) → bikin partisi + indeks per komponen.
  if (!(await HasValidBaseTable())) {
    await PrepareDatabase();
  }
  ApplicationLifecycle.setPhase(ApplicationPhase.DATABASE_READY);
  await ComponentRegistry.registerAllComponents();

  if (!JSON_OUT) console.log(`BunSane — N = ${N}, iters = ${ITERS}\n`);
  const stats: Stat[] = [];

  // 1) save (bulk insert): buat + simpan N entity.
  stats.push(
    await bench("save", N, async () => {
      await truncate();
      const ents = buildEntities();
      await saveAll(ents);
    }),
  );

  // Pastikan DB terisi utk load/filter.
  await truncate();
  await saveAll(buildEntities());

  // 2) load: scan seluruh entity + kedua komponen (eager-load). `.take(N)` agar
  // tak ke-cap BUNSANE_DEFAULT_QUERY_LIMIT (default 10000).
  stats.push(
    await bench("load", N, async () => {
      const ents = await new Query()
        .with(Position)
        .with(Health)
        .take(N)
        .eagerLoad([Position, Health])
        .exec();
      if (ents.length !== N) throw new Error(`load: dapat ${ents.length}, harap ${N}`);
    }),
  );

  // 3) filter: entity dgn Health.hp < 20.
  const filterQ = () =>
    new Query()
      .with(Health, { filters: [{ field: "hp", operator: "<", value: 20 }] })
      .take(N)
      .exec();
  const matched = (await filterQ()).length;
  stats.push(await bench("filter", matched, async () => void (await filterQ())));

  // 4) incremental: muat ~10% entity, ubah hp, save-balik. Setup di luar timing.
  const targets = await new Query()
    .with(Health)
    .take(Math.ceil(N / 10))
    .eagerLoad([Health])
    .exec();
  stats.push(
    await bench("incremental", targets.length, async () => {
      // `set` = update komponen tersimpan (INSERT→UPDATE), lalu save writeback.
      for (const e of targets) {
        const h = e.getCached(Health) as { hp: number } | undefined;
        await e.set(Health, { hp: (h?.hp ?? 0) + 1 });
      }
      await saveAll(targets);
    }),
  );

  if (JSON_OUT) {
    console.log(
      JSON.stringify(
        {
          engine: "bunsane",
          n: N,
          iters: ITERS,
          concurrency: SAVE_CONCURRENCY,
          results: stats.map((s) => ({ ...s })),
        },
        null,
        2,
      ),
    );
  } else {
    for (const s of stats) {
      console.log(
        `  ${s.workload.padEnd(18)} ${s.ms_avg.toFixed(2).padStart(9)} ms  (min ${s.ms_min
          .toFixed(2)
          .padStart(7)})  ${s.per_sec.toFixed(0).padStart(12)} ent/s`,
      );
    }
  }
  await db.end?.();
}

main().then(
  () => process.exit(0),
  (e) => {
    console.error(e);
    process.exit(1);
  },
);
