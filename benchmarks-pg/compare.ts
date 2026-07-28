// Gabungkan hasil JSON kedua engine → tabel perbandingan.
// Pakai: bun compare.ts arke.json bunsane.json
import { readFileSync } from "node:fs";

type Row = { workload: string; n: number; ms_avg: number; per_sec: number };
type Report = { engine: string; n: number; iters: number; results: Row[] };

const [a, b] = process.argv.slice(2);
if (!a || !b) {
  console.error("pakai: bun compare.ts <arke.json> <bunsane.json>");
  process.exit(1);
}
const ra: Report = JSON.parse(readFileSync(a, "utf8"));
const rb: Report = JSON.parse(readFileSync(b, "utf8"));

const byWl = (r: Report) => new Map(r.results.map((x) => [x.workload, x]));
const ma = byWl(ra);
const mb = byWl(rb);
const workloads = [...new Set([...ma.keys(), ...mb.keys()])];

const pad = (s: string, n: number) => s.padEnd(n);
const padL = (s: string, n: number) => s.padStart(n);

console.log(`\n  Perbandingan — N = ${ra.n} (iters ${ra.iters}/${rb.iters})\n`);
console.log(
  `  ${pad("workload", 13)}${padL(ra.engine, 15)}${padL(rb.engine, 12)}   ${"pemenang"}`,
);
console.log(`  ${"-".repeat(70)}`);
for (const wl of workloads) {
  const x = ma.get(wl);
  const y = mb.get(wl);
  const xs = x ? `${x.ms_avg.toFixed(2)} ms` : "—";
  const ys = y ? `${y.ms_avg.toFixed(2)} ms` : "—";
  let winner = "—";
  if (x && y) {
    const r = y.ms_avg / x.ms_avg;
    winner =
      r >= 1
        ? `${ra.engine} ${r.toFixed(2)}× lebih cepat`
        : `${rb.engine} ${(1 / r).toFixed(2)}× lebih cepat`;
  }
  console.log(`  ${pad(wl, 13)}${padL(xs, 15)}${padL(ys, 12)}   ${winner}`);
}
console.log();
console.log("  Catatan: micro-benchmark lintas-bahasa di atas Postgres yang sama.");
console.log("  Angka RELATIF & spesifik-mesin — didominasi round-trip DB & pola query.");
console.log("  Asimetri: BunSane menulis dgn koneksi konkuren (pool), arke sekuensial");
console.log("  dalam 1 transaksi → write-heavy condong ke BunSane, read ke arke.\n");
