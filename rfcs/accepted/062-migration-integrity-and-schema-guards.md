# RFC-062: Migration Integrity and Schema Guards

**Project:** orbok\
**RFC:** 062\
**Title:** Migration Integrity and Schema Guards\
**Status:** Accepted\
**Accepted:** 2026-09-02 by the project owner\
**Target milestone:** on-disk format integrity\
**Date:** 2026-09-01\
**Related RFCs:** RFC-002 SQLite Catalog Schema and Migration Policy (this amends its enforcement, not its design); RFC-036 Resource-Aware Indexing Scheduler (the edit was made for it); RFC-049 Portable Runtime Data Isolation (a profile moved between machines is one way §6's guard bites)

---

## 1. Summary

`crates/data/db/src/migrations.rs:18-19` declares:

> *"The append-only migration list. New migrations are appended here and never
> reordered or edited after release."*

Tag `0.16.0` shipped `0001_baseline.sql` with
`status IN ('queued','running','succeeded','failed','canceled','blocked')`.
Commit `c54e89d` — RFC-036, released in 0.17.0 — **rewrote that line in the
released baseline** to add `'paused'` and `'waiting_for_dependency'`, with no
new migration to rebuild the table.

The justification, in `0003_scheduler.sql:12-15`, is:

> *"Existing databases accept them because SQLite does not re-validate CHECK
> constraints on existing rows after migration."*

That statement is true and does not support the conclusion. SQLite does not
re-validate existing **rows** — and it enforces the stored CHECK on every INSERT
and UPDATE. I confirmed this directly against SQLite. An upgraded catalog keeps
the CHECK it was created with, so a catalog created by orbok ≤ 0.16.0 reaches
`schema_version = 6` cleanly and then **rejects `status='paused'`**.

**User-visible consequence:** on such a catalog, turning off background indexing
saves the setting and silently fails to pause anything, because
`scheduler_host.rs:210` discards the error with `let _ =`.

A second, independent defect on the same seam: `Catalog::from_connection` has no
schema-downgrade guard. A catalog stamped with a **newer** schema version opens
without complaint and queries run against an unknown layout.

---

## 2. Motivation

An on-disk format that a desktop application writes into the user's home
directory has exactly two invariants that matter: **released migrations never
change**, and **a database from the future is refused rather than used**. Both
are currently broken, and the first was broken by a change that passed review
because its justification sounded authoritative.

The failure is silent in both directions. Users on a pre-0.17 catalog cannot
pause indexing and are told nothing. A user who syncs a profile between two
machines running different orbok versions gets the older binary operating on the
newer schema, quietly.

Note also that `run_check` — the headless `--check` diagnostic — **does** compare
the stored version against `latest_version()`. So the diagnostic catches the
condition and the actual application does not. That inversion is worth fixing on
its own.

---

## 3. Goals

- Make the append-only rule true again, in the tree and in CI.
- Repair catalogs that carry the pre-0.17 CHECK.
- Refuse to operate on a schema from the future.
- Delete a comment that misstates how SQLite works, so it cannot be cited again.

## 4. Non-Goals

- A downgrade *migration* path (rewriting a newer schema to an older one). Out
  of scope and probably permanently: refusing is correct and honest.
- Changing RFC-002's migration design. Numbered, append-only, transactional,
  abort-on-failure is right; only its enforcement is missing.
- Data recovery for catalogs already corrupted by a schema mismatch. None are
  known; §6 prevents future ones.

---

## 5. Decision 1 — repair, then restore, then gate

**Three steps, in this order. The order matters: restoring `0001` before the
repair migration exists would break fresh installs.**

**Step 1 — add migration `0007_index_jobs_status_check.sql`.** Rebuild
`index_jobs` with the full CHECK, using SQLite's standard table-rewrite:

```sql
PRAGMA foreign_keys=OFF;
CREATE TABLE index_jobs_new (…full CHECK including 'paused', 'waiting_for_dependency'…);
INSERT INTO index_jobs_new SELECT … FROM index_jobs;
DROP TABLE index_jobs;
ALTER TABLE index_jobs_new RENAME TO index_jobs;
-- recreate the three indexes
PRAGMA foreign_keys=ON;
```

inside the transaction the migration runner already provides. This is idempotent
in the sense that matters: a catalog created after `c54e89d` already has the
wide CHECK and the rebuild is a no-op in behaviour.

**Step 2 — restore `0001_baseline.sql` to its released 0.16.0 text.** After
step 1, fresh installs get the narrow CHECK from `0001` and the wide one from
`0007`, exactly as an append-only list is supposed to work. The append-only rule
becomes true rather than aspirational.

**Step 3 — delete the incorrect claim in `0003_scheduler.sql:12-15`** and
replace it with a note pointing at `0007` and at this RFC. The comment as
written is a correct fact deployed as a wrong justification, which is the most
durable kind of wrong comment.

## 6. Decision 2 — the schema-version guard moves into the application path

Move the version check from `run_check` into `Catalog::from_connection`:

```rust
if stored > migrations::latest_version() {
    return Err(/* typed error naming both versions */);
}
```

Refuse, with an error that names the stored version and the supported version,
so the user is told *"this data directory was written by a newer orbok"* rather
than experiencing arbitrary query failures.

Three real paths reach this: a downgrade, a data directory synced between
machines, and `ORBOK_DATA_DIR` pointed at a newer profile — which RFC-049 and
RFC-054 make a first-class capability, so this is not hypothetical.

## 7. Decision 3 — CI enforces the append-only rule

A rule that depends on remembering it will be broken again. Add a gate:

```sh
# fails if any migration file that existed at the last release tag has changed
git diff --name-only "$LAST_TAG"..HEAD -- crates/data/db/migrations/ \
  | while read -r f; do
      git cat-file -e "$LAST_TAG:$f" 2>/dev/null && echo "released migration edited: $f"
    done
```

Placed in the `fast` job so it fails early and cheaply. New files are unaffected;
only previously-released ones are protected.

**Self-test it.** This project's shell gates (`check-design-tokens.test.sh`,
`check-i18n-literals.test.sh`, `check-rfc-lifecycle.test.sh`,
`package.test.sh`) all test the checker itself, and that practice is the reason
those gates are trustworthy. This one gets the same treatment: a test that
constructs an edit to a released migration and asserts the gate rejects it.

---

## 8. Acceptance criteria

Phrased per RFC-058 §5.

1. A catalog created by orbok 0.16.0, opened by the current binary, reaches the
   latest schema version and then **accepts** an `index_jobs` row with
   `status='paused'`. (Today it rejects it — build the fixture from the 0.16.0
   tag and verify the rejection first.)
2. On that same upgraded catalog, toggling background indexing off actually
   pauses indexing, observable as jobs ceasing to be dispatched.
3. `git diff 0.16.0 HEAD -- crates/data/db/migrations/0001_baseline.sql` is
   empty.
4. A catalog whose `schema_version` is set one above `latest_version()` causes
   the application to refuse to open it with an error naming both versions —
   and `--check` reports the same condition, as it already does.
5. The CI gate fails on a scratch branch that edits any released migration file,
   and passes on one that adds a new migration file. Verified by pushing both,
   not by inspection.
6. The gate's own self-test fails if the gate is made to always succeed.

---

## 9. Open questions

1. **How is `$LAST_TAG` determined in CI?** `git describe --tags --abbrev=0` on
   a shallow clone needs `fetch-depth: 0`. Alternative: pin the comparison to a
   recorded tag in the script and bump it at release. The second is uglier and
   has no failure mode; the first is cleaner and fails confusingly when the
   checkout is shallow. Implementer's call, documented in the script's header.
2. **Should `0007` also repair anything else edited post-release?** The audit
   found one edit. A full `git log -p` over `migrations/` since 0.1.0 should be
   run once as part of this work, so that "one edit" is a verified statement and
   not an assumption. If others exist, they join `0007` or get their own numbers.
3. **Does the guard belong in `from_connection` or in `open`?** `from_connection`
   is also used by `open_in_memory` for tests, where the check is a no-op.
   Harmless either way; note it so it is a choice rather than an accident.
