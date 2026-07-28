// Susun hasil sweep konkurensi → tabel scaling (ms_avg) per workload,
// baris per engine, kolom per level konkurensi. Pakai: bun sweep_report.ts <json...>
import { readFileSync } from "node:fs";

type Row = { workload: string; n: number; ms_avg: number; per_sec: number };
type Report = { engine: string; n: number; iters: number; concurrency: number; results: Row[] };

const reports: Report[] = process.argv.slice(2).map((p) => JSON.parse(readFileSync(p, "utf8")));
if (reports.length === 0) {
  console.error("pakai: bun sweep_report.ts <json...>");
  process.exit(1);
}

const engines = [...new Set(reports.map((r) => r.engine))];
const levels = [...new Set(reports.map((r) => r.concurrency))].sort((a, b) => a - b);
const workloads = [...new Set(reports.flatMap((r) => r.results.map((x) => x.workload)))];
const N = reports[0].n;

// lookup: engine → concurrency → workload → Row
const idx = new Map<string, Row>();
for (const r of reports)
  for (const x of r.results) idx.set(`${r.engine}|${r.concurrency}|${x.workload}`, x);

const padR = (s: string, n: number) => s.padEnd(n);
const padL = (s: string, n: number) => s.padStart(n);
const W = 12;

console.log(`\n  Sweep konkurensi — N = ${N}  (angka = ms rata-rata, makin kecil makin baik)\n`);

for (const wl of workloads) {
  const flat = wl === "load" || wl === "filter";
  console.log(`  ${wl}${flat ? "  (baca: query tunggal, ~independen konkurensi)" : ""}`);
  // header
  let head = padR("    engine", 18);
  for (const c of levels) head += padL(`C=${c}`, W);
  head += padL("scaling", W + 2);
  console.log(head + "\n    " + "-".repeat(head.length - 4));
  for (const e of engines) {
    let line = padR(`    ${e}`, 18);
    const vals: (number | null)[] = [];
    for (const c of levels) {
      const row = idx.get(`${e}|${c}|${wl}`);
      vals.push(row ? row.ms_avg : null);
      line += padL(row ? row.ms_avg.toFixed(1) : "—", W);
    }
    // scaling = ms@min-level / ms@max-level (>1 → makin cepat saat konkurensi naik)
    const first = vals[0];
    const last = vals[vals.length - 1];
    const sc = first != null && last != null && last > 0 ? `${(first / last).toFixed(2)}×` : "—";
    line += padL(sc, W + 2);
    console.log(line);
  }
  console.log();
}

console.log("  Kolom 'scaling' = ms(C-terkecil) / ms(C-terbesar): >1 berarti workload");
console.log("  memanfaatkan konkurensi/multi-core. Tulis (save/incremental) mestinya");
console.log("  naik; baca (load/filter) datar. Lintas-bahasa di Postgres yg sama →");
console.log("  angka RELATIF, spesifik-mesin, didominasi round-trip DB.\n");
