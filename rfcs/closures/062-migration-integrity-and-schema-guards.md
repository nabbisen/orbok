# Closure Record — RFC-062: Migration Integrity and Schema Guards

**RFC:** [062](../done/062-migration-integrity-and-schema-guards.md)
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** `rfcs/handoffs/HANDOFF-062-migration-integrity-and-schema-guards.md`.
Commits: `125ba7e` (migration `0007`, `0001_baseline.sql` restored, the
guard moved into `Catalog::from_connection`, the CI gate and its
self-test), `b85faa9` (provisional allowlist entry for the repair itself),
`cd0d445` (the shrink-only check compares `HEAD` with its parent, not with
the index), `3878420` (allowlist emptied once 0.25.0 shipped both files).
**Transcribed, not re-derived**, from what was run on 2026-09-16 at
`82c1415` (Task 052 closure sweep).

---

## §8 acceptance criteria

### 1. A catalog created by orbok 0.16.0, opened by the current binary, reaches the latest schema version and then **accepts** an `index_jobs` row with `status='paused'`.

→ what was run: `criterion_1_a_0_16_0_catalog_rejects_paused_before_upgrade_and_accepts_it_after`
(`crates/data/db/src/tests/rfc062_migration_integrity.rs`). The fixture is
built from `0001_baseline.sql`'s own restored text and stamped at version
1. The test asserts the CHECK rejection **before** the upgrade, as the
criterion asks, and acceptance after `Catalog::open`.
→ what was observed: `ok` (1 passed).
→ where verified: `cargo test -p orbok-db rfc062`.

### 2. On that same upgraded catalog, toggling background indexing off actually pauses indexing, observable as jobs ceasing to be dispatched.

→ what was run: `criterion_2_pausing_background_indexing_actually_pauses_an_upgraded_0_16_0_catalog`
(`crates/app/src/rfc062_acceptance_tests.rs`). The same kind of 0.16.0
fixture is upgraded. Then `Scheduler::pause` must move a real queued job
out of `queued`, the state the dispatch loop selects from.
→ what was observed: `ok`.
→ where verified: `cargo test -p orbok --bin orbok rfc06`.

### 3. `git diff 0.16.0 HEAD -- crates/data/db/migrations/0001_baseline.sql` is empty.

→ what was run: `git diff --stat 0.16.0 HEAD -- crates/data/db/migrations/0001_baseline.sql`.
→ what was observed: no output, exit 0.

### 4. A catalog whose `schema_version` is set one above `latest_version()` causes the application to refuse to open it with an error naming both versions — and `--check` reports the same condition, as it already does.

→ what was run, unit: `catalog::tests::schema_version_from_the_future_is_refused_naming_both_versions`
(`crates/data/db/src/catalog.rs`).
→ observed: `ok`.
→ what was run, the real binary:
1. `orbok --check` against a fresh `ORBOK_DATA_DIR` created a catalog at
   `schema_version=8`.
2. `sqlite3` inserted a `schema_migrations` row with version 9.
3. `orbok --check` ran again.
4. `orbok` with no arguments ran with `WAYLAND_DISPLAY`/`DISPLAY` unset, on
   the same directory.

→ observed: both paths printed
`Error: SchemaVersionUnsupported { stored: 9, supported: 8 }` and exited 1.
The GUI path refused before any window was attempted.

### 5. The CI gate fails on a scratch branch that edits any released migration file, and passes on one that adds a new migration file. Verified by pushing both, not by inspection.

→ what was run: the two pushes this criterion asks for happened on `main`
itself rather than on scratch branches. Review 212 §2 accepted that as
meeting the criterion. It is recorded here as a deviation from the
wording, not a reinterpretation of it.
- CI run `34418057815` (`125ba7e`): the Fast gate failed with
  `migration-integrity gate: released migration edited since 0.24.0:
  crates/data/db/migrations/0001_baseline.sql (not on the allowlist …)`.
  This is the "rejects an edit" half.
- CI run `34418504542` (`b85faa9`): success, with the same commit series
  adding the new `0007_index_jobs_status_check.sql`, which the gate did not
  flag. This is the "passes a new migration" half.

→ observed: both conclusions and the failure line were re-read with
`gh run view` on 2026-09-16.
→ also run locally: `bash scripts/check-migration-integrity.test.sh`
printed `ok: a new migration file: passes` and
`ok: editing a released migration: fails`, among 10 cases, and ended with
`check-migration-integrity.test.sh: ok`. `bash scripts/check-migration-integrity.sh`
printed `migration-integrity gate: ok`.

### 6. The gate's own self-test fails if the gate is made to always succeed.

→ what was run: `scripts/check-migration-integrity.test.sh` was copied to
a scratch directory outside the working tree, next to a gutted
`check-migration-integrity.sh` (`echo ok; exit 0`), and run there. The
self-test resolves the gate relative to its own location, so the real
tree was never modified.
→ what was observed: 12 `FAIL:` lines, including
`editing a released migration: fails (expected fail, got pass)`, and
`check-migration-integrity.test.sh: failed`, exit 1.

---

## Criteria not met, and why RFC-062 closes anyway

None. Criterion 5's evidence is two pushes to `main` rather than to
scratch branches. That was accepted in Review 212 §2 and is disclosed
under criterion 5 above.
