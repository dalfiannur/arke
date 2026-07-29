# Pid-Based Relations (RFC-0034 Amendment 3) — Implementation Plan

> **For agentic workers:** execute task-by-task; each task ends green (`cargo test` / clippy). Steps use `- [ ]`.

**Goal:** Make entity relations (RFC-0031/0032) native to `pid`, un-ignoring the 5 deferred relation tests, targeting **arke-postgres 0.13.0**.

**Architecture:** One `<rel>_id` column stores the referenced **pid** (drop `_gen`). New `PgValue::Ref(i64)` carries a World **index** in-memory; the store maps `Ref(index)→pid` on **write** and `pid→Ref(local_index)` on **read** (via `pid_of`/`entity_of`). `from_params` stays pure: loaded worlds are fresh (`generation==0`) so it reconstructs `Entity::from_raw(index, 0)`. No FK; `ColumnDef.entity_ref` flag marks relation columns for the read/select path. Dangling ref (target not co-loaded) → `Null`.

**Tech Stack:** Rust, arke + arke-postgres, sqlx/Postgres (podman `arke-pid-pg` :55432).

**Test DB:** recreate clean before DB runs — `podman exec arke-pid-pg psql -U postgres -c "DROP DATABASE IF EXISTS arke_test WITH (FORCE);" -c "CREATE DATABASE arke_test;"`. Run relation/integration tests with `DATABASE_URL=postgres://postgres:postgres@localhost:55432/arke_test cargo test -p arke-postgres -- --test-threads=1`.

---

## Task 1: `PgValue::Ref` + `ColumnDef.entity_ref`

**Files:** Modify `arke-postgres/src/lib.rs`.

- [ ] **Step 1:** Add variant to `PgValue` (after `Json`): `/// Referensi entity (RFC-0034 Am.3): membawa **indeks** World saat dump; store memetakan ke/dari pid di batas DB.` `Ref(i64),`.
- [ ] **Step 2:** Add field to `ColumnDef`: `/// Kolom relasi entity (RFC-0034 Am.3): nilai = pid entity yang dirujuk; jalur baca menerjemahkan pid↔Ref.` `pub entity_ref: bool,`. Update the `scalar()` const helper to set `entity_ref: false`.
- [ ] **Step 3:** `cargo check -p arke-postgres` — expect errors in `store.rs`/`query.rs` (bind_value non-exhaustive, ColumnDef literals). Fixed in later tasks. Then `cargo check` after Task 2–4.

## Task 2: derive — single pid column, `Ref`, pure `from_params`

**Files:** Modify `arke-postgres-derive/src/lib.rs` (relation branch ~305-367; `column_def` helper).

- [ ] **Step 1:** In the relation branch: emit ONE column `column_def_ref("{name}_id", nullable)` (new helper producing a `ColumnDef` literal with `entity_ref: true`, `references: None`, `ty: BigInt`). Set `slots: 1`.
- [ ] **Step 2:** `to_param`:
  - non-nullable: `::arke_postgres::PgValue::Ref({acc_self}.index() as i64), `
  - nullable: `match &self.{name} {{ Some(e) => ::arke_postgres::PgValue::Ref({acc_e}.index() as i64), None => ::arke_postgres::PgValue::Null }}, `
- [ ] **Step 3:** `from_field` (single value at `{idx}`; `ent = ::arke::Entity::from_raw(*i as u32, 0)`; `mk` wraps `Ref::new` iff typed):
  - non-nullable: `{name}: match values.get({idx}) {{ Some(::arke_postgres::PgValue::Ref(i)) => {mk}, _ => return None }}, `
  - nullable: `{name}: match values.get({idx}) {{ Some(::arke_postgres::PgValue::Null) => None, Some(::arke_postgres::PgValue::Ref(i)) => Some({mk}), _ => return None }}, `
- [ ] **Step 4:** Update the `column`/token-generation site (~488) that computed the relation column name — it already uses `{name}_id`; ensure the running value-index (`val_idx`) advances by 1 (was 2) via `slots: 1`.
- [ ] **Step 5:** `cargo test -p arke-postgres --test derive` (no DB) — update `ref_bertipe_kolom_token_dan_round_trip` / any derive test asserting 2 columns or `_gen` to the single-column pid shape. Expect green.

## Task 3: store write paths — resolve `Ref(index)→pid`

**Files:** Modify `arke-postgres/src/store.rs`.

- [ ] **Step 1:** Add helper on `PgStore`:
  ```rust
  /// Ganti `PgValue::Ref(index)` → `Int(pid)` (RFC-0034 Am.3). Ref menggantung
  /// (indeks tak ter-map, mis. per-op) → `Null`.
  fn resolve_refs(&self, params: &[PgValue]) -> Vec<PgValue> {
      params.iter().map(|v| match v {
          PgValue::Ref(idx) => match self.pid_of.get(&(*idx as u32)) {
              Some(pid) => PgValue::Int(*pid),
              None => PgValue::Null,
          },
          other => other.clone(),
      }).collect()
  }
  ```
- [ ] **Step 2:** In every write loop that binds a component row from owned params — `commit` (full save), `commit_insert`, `commit_update`, `commit_incremental`, `update_entity` (uses `dump_one` live) — resolve before binding. For owned-param loops: `let params = self.resolve_refs(&params);` before `for (value, col) in params.iter().zip(r.columns)`. For `update_entity`'s `(r.dump_one)(world, entity)` path: `let params = self.resolve_refs(&params);`. Ensure pid allocation for ALL working-set entities happens BEFORE resolve (already true in `commit`/`commit_incremental`; `stage`/`stage_incremental` dump `Ref(index)` unresolved — resolution is commit-side).
- [ ] **Step 3:** Add `bind_value` arm for safety: `PgValue::Ref(i) => q.bind(*i),` (should not reach here post-resolve, but keeps the match exhaustive).
- [ ] **Step 4:** `cargo check -p arke-postgres`.

## Task 4: store read paths — translate `pid→Ref(local_index)`

**Files:** Modify `arke-postgres/src/store.rs` (`materialize` apply sites; `fetch`).

- [ ] **Step 1:** Add helper:
  ```rust
  /// Untuk kolom `entity_ref`: `Int(pid)` → `Ref(indeks lokal)` via `entity_of`;
  /// pid tak ter-muat (menggantung) → `Null`. Kolom lain apa adanya.
  fn translate_refs(&self, r: &Registered, values: &mut [PgValue]) {
      for (v, col) in values.iter_mut().zip(r.columns) {
          if col.entity_ref {
              *v = match v {
                  PgValue::Int(pid) => match self.entity_of.get(pid) {
                      Some(e) => PgValue::Ref(i64::from(e.index())),
                      None => PgValue::Null,
                  },
                  _ => PgValue::Null,
              };
          }
      }
  }
  ```
- [ ] **Step 2:** In `materialize`, at BOTH apply sites (cache-hit decode path and DB-miss read path), call `self.translate_refs(r, &mut values)` immediately before `(r.apply)(world, entity, &values)`. Cache stores the **untranslated** (pid) values → translation stays post-decode (do NOT translate before `to_cache.push`).
- [ ] **Step 3:** In `fetch` (per-op get), same translate before applying components.
- [ ] **Step 4:** `cargo check -p arke-postgres --tests`.

## Task 5: `query.rs` — joins/recursion/paths key on `pid`

**Files:** Modify `arke-postgres/src/query.rs`.

- [ ] **Step 1:** `join_cond`: `"{rel_column} IN (SELECT pid FROM {related_table} WHERE {filter_sql})"`.
- [ ] **Step 2:** `target_load_sql`: `SELECT DISTINCT {rel} AS pid ...`.
- [ ] **Step 3:** `PathLoad::load_all`: `root_sql` → `SELECT pid FROM {root_table} ... ORDER BY pid`; `matched_prev` → `SELECT pid FROM {root_table} ...`; per-hop targets → `SELECT DISTINCT {rel} AS pid FROM {from} WHERE pid IN ({matched_prev}) AND {rel} IS NOT NULL`.
- [ ] **Step 4:** `recursive_sql`: replace every `entity_id` with `pid`; join `t.{rel} = rec.pid`; ancestors seed `WHERE pid = ? AND {rel} IS NOT NULL`, recursive join `JOIN rec ON t.pid = rec.pid`, `SELECT {rel} AS pid`.
- [ ] **Step 5:** `Recursive::max_depth` seed param: `root.index()` → `self.store.pid_of.get(&self.root.index()).copied().unwrap_or(0)` (root's pid).
- [ ] **Step 6:** Un-`#[ignore]` unit `matches_bersarang_3_deep`; update its expected SQL + `join_subquery_terparameterisasi` + `recursive_sql_descendants_dan_ancestors` expectations to `pid`.
- [ ] **Step 7:** `cargo test -p arke-postgres --lib` (no DB) green.

## Task 6: un-ignore + rewrite relation integration tests (pid semantics)

**Files:** Modify `arke-postgres/tests/{relations,nested,recursive,typed_relations}.rs`.

- [ ] **Step 1:** Remove the `#[ignore = "relasi …"]` line from each of the 4 files.
- [ ] **Step 2:** Rewrite each so post-`load` verification does NOT assume save-side handles equal loaded handles. Load full world (or root-anchored), then verify relations resolve WITHIN the loaded world: iterate loaded entities, follow the `.pet`/`.boss`/`Ref` field to another loaded entity, assert the target's components. Assert by **content**, not save-side `Entity` equality.
- [ ] **Step 3:** Recreate DB clean, run each: `DATABASE_URL=… cargo test -p arke-postgres --test relations --test nested --test recursive --test typed_relations -- --test-threads=1`. All green.

## Task 7: full green + version bump + commit

- [ ] **Step 1:** Recreate DB clean; run full suite + examples single-threaded — all green, **0 ignored** relation tests remaining.
- [ ] **Step 2:** `cargo clippy -p arke-postgres --tests --examples` clean.
- [ ] **Step 3:** Bump `arke-postgres/Cargo.toml` → `0.13.0`; `arke-cache` dep → `^0.13`; update backend-rs comment `0.12`→`0.13`.
- [ ] **Step 4:** `cargo check --workspace` in backend-rs (downstream builds).
- [ ] **Step 5:** Commit `release: arke-postgres 0.13.0 — relasi berbasis pid (RFC-0034 Am.3)`. Do not push (await user).
