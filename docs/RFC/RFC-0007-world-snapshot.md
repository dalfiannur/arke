# RFC-0007: Snapshot & serialisasi World

- **Status:** Accepted <!-- Draft | Discussion | Accepted | Rejected | Superseded by RFC-XXXX -->
- **Tanggal:** 2026-07-27
- **Milestone:** M-6 (World Snapshot)
- **ADR terkait:** [ADR-0007](../ADR/ADR-0007-world-snapshot.md)

## Ringkasan

Menambahkan kemampuan meng-*snapshot* keadaan `World` ke format terbuka (JSON) yang **berversi** dan memulihkannya secara setia. Serialisasi memakai trait **`Serialize` milik rust-ecs** (tanpa dependensi eksternal) yang di-*opt-in* per tipe komponen lewat `register_serializable::<T>()`. Ini mengaktifkan **STD-0001** (versi pada format) dan **STD-0002** (round-trip setia), mewujudkan invarian *kepemilikan & portabilitas data*.

## Motivasi

ARCHITECTURE_BIBLE §2 & §4 menetapkan bahwa keadaan `World` harus dapat di-snapshot ke format terbuka dan dipulihkan tanpa kehilangan makna. Ini menopang save/replay/rollback dan menjadikan data pengguna portabel (berguna di luar aplikasi). STANDARDS STD-0001/0002 menuntutnya secara terukur.

Kendala: penyimpanan komponen bersifat **type-erased** (`Box<dyn Column>`), dan **serde adalah dependensi eksternal** yang melanggar *standalone core* (STD-0003). Karena itu serialisasi harus (a) tanpa dep, dan (b) memperoleh cara menserialisasi `T` sembarang lewat mekanisme yang ditangkap saat registrasi.

## Usulan rinci

### 1. Representasi terbuka: `Value`

Enum bebas-dependensi yang menjadi bentuk perantara antara komponen dan JSON:

```rust
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    List(Vec<Value>),
    Map(Vec<(String, Value)>),
}
```

`Value` dapat di-*encode* ke/di-*parse* dari **JSON** lewat implementasi tulis-tangan (tanpa dep), mengikuti semangat validator repo (`docs/validators/`).

### 2. Trait `Serialize`

```rust
pub trait Serialize: Component + Sized {
    fn to_value(&self) -> Value;
    fn from_value(value: &Value) -> Option<Self>;
}
```

Komponen yang ingin ikut snapshot mengimplementasikannya. `from_value` mengembalikan `Option` (parse bisa gagal → ditolak, bukan panik).

### 3. Registrasi serializable (opt-in)

```rust
impl World {
    pub fn register_serializable<T: Serialize>(&mut self);
}
```

Menyimpan di registry, per tipe: **nama tipe stabil** (kunci portabel, bukan `ComponentId` numerik — sesuai §4) beserta vtable `to_value`/`from_value` type-erased. Hanya komponen yang terdaftar yang muncul di snapshot; sisanya dilewati (dan didokumentasikan).

### 4. Bentuk snapshot

```rust
pub struct Snapshot { /* schema_version + entities */ }
```

Struktur logis (entity-centric, portabel):

```json
{
  "schema_version": 1,
  "entities": [
    { "index": 0, "generation": 0,
      "components": { "Position": {"x": 1, "y": 2}, "Velocity": {"x": 0, "y": 0} } }
  ]
}
```

- **`schema_version`** wajib (STD-0001).
- Entity direkam beserta `index`+`generation` agar handle tetap valid setelah restore (round-trip setia).
- Komponen dikunci oleh **nama tipe** (portabel lintas-proses).

### 5. API

```rust
impl World {
    pub fn snapshot(&self) -> Snapshot;                 // World → Snapshot
    pub fn load_snapshot(&mut self, snap: &Snapshot);   // Snapshot → World (tipe harus sudah teregistrasi)
}
impl Snapshot {
    pub fn to_json(&self) -> String;                    // Snapshot → teks JSON (berisi schema_version)
    pub fn from_json(json: &str) -> Option<Snapshot>;   // teks JSON → Snapshot
}
```

Round-trip setia (STD-0002): untuk `World` yang komponennya semua terdaftar-serializable, `load_snapshot(&world.snapshot())` menghasilkan `World` yang setara secara observasional (entity hidup, komponen, nilai identik lewat `get`/`query`).

### 6. Schema & konformansi

JSON schema `schema/v1/world-snapshot.schema.json` mendeskripsikan format, menandai `schema_version` sebagai `required` (STD-0001 verify), dengan contoh valid/tak-valid mengikuti pola `docs/schema/examples/`.

## Alternatif yang dipertimbangkan

| Alternatif | Kelebihan | Kekurangan | Mengapa tidak dipilih |
| --- | --- | --- | --- |
| serde (feature opsional) | Ergonomis, ekosistem-standar | Dependensi eksternal, feature-gating | Melanggar semangat standalone; trait sendiri cukup |
| Bound `Serialize` pada `Component` | Otomatis, tak perlu registrasi | Memaksa semua komponen serializable | Terlalu membatasi; opt-in lebih fleksibel |
| Snapshot component-centric (per-archetype) | Dekat layout internal | Kurang portabel/terbaca; bocorkan detail archetype | Entity-centric lebih portabel (invarian portabilitas) |
| Kunci komponen dgn `ComponentId` numerik | Ringkas | Tak stabil lintas-proses | Nama tipe stabil (§4) |

## Dampak

- **Kompatibilitas / migrasi:** aditif. Format diberi versi (`schema_version`); perubahan format kelak butuh migrasi eksplisit + versi baru (ARCHITECTURE_BIBLE §6.1).
- **Keamanan / provenance:** snapshot terbuka & terbaca di luar aplikasi (portabilitas). Tetap tanpa `unsafe`.
- **Konsekuensi pada invarian:** mengaktifkan STD-0001 & STD-0002; memperkuat *kepemilikan & portabilitas data* dan *standalone* (STD-0003, tanpa serde).

## Pertanyaan terbuka

- Round-trip *free-list* & generasi mati (entity yang sudah despawn) — untuk M-6, hanya entity hidup di-snapshot; rekonstruksi free-list persis ditunda. → RN bila perlu.
- Migrasi antar-`schema_version` (mis. v1 → v2) → milestone saat versi kedua muncul.
- Tipe komponen bersarang/koleksi kompleks di `Value` → `Map`/`List` menutup kasus umum; tipe eksotis tanggung jawab `Serialize` pengguna.

## Keputusan

Diterima. Lihat [ADR-0007](../ADR/ADR-0007-world-snapshot.md).
