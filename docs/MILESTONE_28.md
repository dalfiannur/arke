# Milestone 28 — Relasi bersarang: `matches` nesting (RFC-0032 fase 1)

> Penyaringan relasi **3–4 deep** (rantai heterogen) via predikat `matches` yang
> bersarang. Aditif; jalan dengan relasi `Entity` yang ada (tanpa `Ref<T>`).

## Ruang lingkup

**Termasuk:** `Field<C, EntityRef>::matches<R>(Filter<R>) -> Filter<C>` (sub-query
bersarang); `join` di-reframe jadi `filter(rel.matches(f))`.

**Tidak termasuk (fase lanjut):** `Ref<T>` bertipe, path builder + deep `join_load`
(M-29), recursive CTE (M-30).

## Definition of Done

- [ ] Unit SQL-gen: `matches` 3-deep → sub-query bersarang ter-parameterisasi.
- [ ] Integrasi DB: rantai relasi 3-deep menyaring benar.
- [ ] fmt/clippy/CI hijau; RFC-0031 API tak berubah perilaku.
