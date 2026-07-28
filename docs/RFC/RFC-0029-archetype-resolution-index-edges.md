# RFC-0029: Resolusi archetype O(1) — index lookup + edge transisi

- **Status:** **Rejected** — premis dibantah pengukuran (lihat "Hasil & keputusan")
- **Tanggal:** 2026-07-29
- **Graduasi dari:** [RN-0003](../RN/RN-0003-competitive-benchmark.md) (celah spawn tersisa)
- **ADR terkait:** [ADR-0029](../ADR/ADR-0029-archetype-resolution-index-edges.md)

> **Hasil & keputusan (2026-07-29): DITOLAK setelah diimplementasi & diukur.**
> Implementasi lengkap (index + edge) lolos uji model-based oracle + miri, tapi
> A/B (git-stash, benchmark W6) membantah premisnya: **resolusi archetype BUKAN
> bottleneck spawn — alokasi yang dominan.** Scan linear OLD di **4096** archetype
> (~62 ns/op) ≈ sama dengan di 256 (~68 ns/op): membandingkan slice-id pendek
> murah & branch-predicted, tak tumbuh berarti. Kemenangan (~62→~43) terkubur
> noise mesin (NEW berayun 43–75). Sesuai YAGNI + "ukur, jangan tebak" (RN-0003),
> implementasi **di-revert**; kompleksitas (2 HashMap/World + edge) tak sepadan
> untuk win tak-terukur. Temuan dicatat di [RN-0003](../RN/RN-0003-competitive-benchmark.md).
> Dokumen ini disimpan sebagai catatan negatif (apa yang dicoba & mengapa ditolak).

## Ringkasan

Mengganti **scan linear** pada `find_or_create_archetype` (`world.rs:622` —
membandingkan seluruh slice `ComponentId` untuk **setiap** archetype tiap
spawn/insert/remove) dengan **index hash** `himpunan-komponen → archetype`
(**O(1)**), ditambah **cache edge transisi** `(archetype, ±komponen) → archetype`
untuk insert/remove tunggal (menghindari konstruksi+sort vektor id berulang).
Internal-only — tanpa perubahan API atau perilaku.

## Motivasi

`find_or_create_archetype` melakukan `archetypes.iter().position(|a| a.ids == ids)`
— **O(n_archetypes × arity)** per mutasi struktural. Untuk world dengan **banyak
archetype** (game nyata: ratusan kombinasi komponen), biaya spawn/insert tumbuh
**super-linear**. Ini sisa celah dari RN-0003 (spawn ~1.8× hecs).

**Kunci keamanan:** archetype di arke **append-only** — `despawn`/`remove` tak
pernah menghapus archetype; `find_or_create` hanya `push`. Maka index & edge
**tak pernah basi** → tanpa logika invalidasi (risiko rendah).

## Usulan rinci

### 1. Index lookup (inti)

Field baru `World`: `archetype_index: HashMap<Box<[ComponentId]>, usize, FxBuild>`.

```rust
fn find_or_create_archetype(&mut self, ids: &[ComponentId]) -> usize {
    if let Some(&i) = self.archetype_index.get(ids) {   // O(1), Box<[_]>: Borrow<[_]>
        return i;
    }
    let columns = ids.iter().map(|&id| self.registry.new_column(id)).collect();
    self.archetypes.push(Archetype::new(ids.to_vec(), columns));
    let idx = self.archetypes.len() - 1;
    self.archetype_index.insert(ids.into(), idx);
    idx
}
```

Hasher **FxHash** cepat (0-dependensi, senapas [RFC-0024](RFC-0024-fast-component-resolution.md)) atas byte slice id — SipHash default terlalu mahal untuk jalur per-mutasi.

### 2. Edge transisi (refinemen insert/remove tunggal)

Field baru `World`: `add_edges: HashMap<(usize, ComponentId), usize, FxBuild>` &
`remove_edges: …`. Pada `insert::<T>`/`remove::<T>` (satu komponen):

```rust
// insert cid ke entity di archetype `a`:
let dst = match self.add_edges.get(&(a, cid)) {
    Some(&d) => d,                                   // hit: lompat, tanpa bangun ids
    None => {
        let mut ids = self.archetypes[a].component_ids().to_vec();
        ids.push(cid); ids.sort_unstable();
        let d = self.find_or_create_archetype(&ids);
        self.add_edges.insert((a, cid), d);          // permanen (append-only)
        d
    }
};
```

Menghindari alokasi+sort vektor id pada transisi berulang (mis. churn: sisip lalu
hapus `Tag` ribuan kali). `insert_bundle` (multi-komponen) tetap lewat index (§1).

### 3. Determinisme

Index & edge **hanya untuk lookup**; vektor `archetypes` tetap **push-order**
(STD-0005). Urutan iterasi/hasil tak berubah. Diverifikasi uji model-based (oracle)
+ miri.

## Hasil yang diharapkan & pengukuran jujur

- **Micro-benchmark existing (W1–W5)**: sedikit/tak berubah — memakai **1–16
  archetype**, jadi scan linear sudah murah; index bahkan menambah overhead hash
  kecil. **Bukan** target optimasi ini.
- **Skala (banyak archetype)**: benchmark **baru W6** — spawn/insert lintas **ratusan
  archetype berbeda** — menunjukkan O(1) vs O(n): di sinilah kemenangannya.
- **Churn (W4)**: edge memangkas alokasi id-vec berulang.

Klaim akan **diukur, bukan ditebak** (pelajaran RN-0003). Bila W6 tak menunjukkan
kemenangan bermakna, temuan dicatat jujur.

## Dampak

- **Kompatibilitas:** internal murni — API & hasil identik. Aditif (pasca-1.0 aman).
- **Memori:** dua HashMap kecil per `World` (proporsional jumlah archetype/edge).
- **Keamanan/determinisme:** tak terpengaruh; append-only → tanpa invalidasi.

## Alternatif yang dipertimbangkan

| Alternatif | Mengapa tidak |
| --- | --- |
| Biarkan scan linear | Degradasi O(n_archetypes) di world kompleks |
| Index tanpa edge | Edge memangkas alokasi id-vec pada churn; murah ditambah (append-only) |
| SipHash default | Terlalu mahal untuk jalur per-mutasi; FxHash 0-dep sudah ada |
| Edge di dalam `Archetype` (interior mut) | Map di `World` lebih sederhana, tanpa `RefCell` |

## Keputusan

Diterima. Lihat [ADR-0029](../ADR/ADR-0029-archetype-resolution-index-edges.md).
