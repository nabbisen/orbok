# Implementation Handoff — RFC-062: Migration Integrity and Schema Guards

**Project:** orbok\
**RFC:** 062\
**Lifecycle stage:** Accepted 2026-09-02. Unstarted. §9 open question 2's sweep is done — §1 below is its result.\
**Primary owner:** `crates/data/db/migrations/`, `.../src/catalog.rs`, `scripts/`, `.github/workflows/ci.yml`\
**RFC:** [`../accepted/062-migration-integrity-and-schema-guards.md`](../accepted/062-migration-integrity-and-schema-guards.md)

> **Scope rule:** Repair the format and guard it. This RFC does **not** change
> RFC-002's migration *design* — numbered, append-only, transactional,
> abort-on-failure is right. Only its enforcement is missing. If a slice tempts
> you into changing how migrations are discovered or ordered, stop.

---

## 1. The sweep RFC-062 §9 Q2 asked for — done, and it found a second file

The RFC assumed the audit had found them all. **It had not.**

Every migration, checked at every release tag against its blob at HEAD:

| migration | first tag | edited after release? | what changed |
|---|---|---|---|
| `0001_baseline.sql` | 0.9.9 | **YES** | **Semantic.** `c54e89d` widened the `index_jobs` status CHECK. This is the audit's finding and §3's repair target. |
| `0002_trigram_index.sql` | 0.9.9 | no | — |
| `0003_scheduler.sql` | 0.17.0 | **YES** | **Comment-only.** `7a9605c` (Task 034 §10) replaced three `--` lines with five, correcting the false SQLite claim. **Zero SQL statements changed.** |
| `0004_search_history.sql` | 0.21.0 | no | — |
| `0005_keyword_rowid_indexes.sql` | 0.23.0 | no | — |
| `0006_managed_model_generations.sql` | *unreleased* | n/a | not in any tag yet, so not yet subject to the rule |

**A method warning, because I got this wrong three times before it was right.**
`git rev-parse "$tag:$path"` **prints the input string to stdout and exits 0**
when the path does not exist, so a naive truthiness check treats every file as
present in every tag. And in this project's shell, unbraced `$t:crates/...`
is parsed as a parameter modifier and silently yields nothing. Brace it —
`${t}:${path}` — and check the exit code, not the output. **Put a self-check in
front of any sweep you write** (mine asserts `0003` is absent at 0.16.0 and
present at 0.17.0, and aborts if not); an unvalidated sweep produced two
different wrong answers before the validated one.

### 1.1 The comment-only edit is genuinely inert, and I checked rather than assumed

`crates/data/db/src/migrations.rs` contains **no hashing of any kind** (`grep -ci
'hash|sha|checksum'` → 0), and `schema_migrations` stores only
`version`, `name`, `applied_at` — **no content digest**. An already-applied
migration is never re-read, and nothing verifies its bytes.

So `0003`'s comment change cannot affect any catalog, upgraded or fresh. It is a
violation of the rule **as literally written** — *"never reordered or edited
after release"* — and of nothing else.

## 2. What §1 means for §7's CI gate — the one real design decision here

A byte-level "released migration files never change" gate **fails on today's tree**,
because `0003`'s bytes changed. Three ways out:

| | |
|---|---|
| **(a) Byte-level + a shrink-only allowlist** | *Recommended.* One grandfathered entry for `0003`, with its reason inline. Same shape as `rfcs/closures/LEGACY-ALLOWLIST.txt`, which already works and self-tests. |
| (b) Semantic — ignore comment-only diffs | More permissive and much harder to get right: "comment-only" is a judgment a shell script makes badly, and a `--` inside a string literal breaks a naive stripper. It also weakens the rule to "don't change the *meaning*", which is not what RFC-002 says. |
| (c) Revert `0003`'s comment | Rejected. The old comment is **factually wrong** about SQLite and Task 034 §10 corrected it deliberately. Reinstating a false claim to satisfy a gate is the wrong trade. |

**Take (a).** The allowlist entry must name what it exempts and why — that this
one edit is comment-only, that nothing hashes migrations, and that the exemption
is not a licence for further edits to that file. Shrink-only, enforced the same
way: compare the staged id set against `HEAD`'s on every commit.

**Self-test it**, as every shell gate here does: construct an edit to a released
migration and assert rejection; construct a *new* migration and assert it passes;
add an id to the allowlist and assert rejection; and confirm the self-test fails
when the check is gutted.

## 3. §5 — the repair, in the order the RFC gives

**Step 1 — `0007_index_jobs_status_check.sql`.** The divergence, confirmed at
line 208 of `0001_baseline.sql`:

```sql
-- what upgraded catalogs still have (0.16.0):
status IN ('queued','running','succeeded','failed','canceled','blocked')
-- what fresh installs get (HEAD):
status IN ('queued','running','succeeded','failed','canceled','blocked','paused','waiting_for_dependency')
```

Rebuild `index_jobs` with the full CHECK using SQLite's table-rewrite, inside the
transaction the runner already provides. **The three indexes to recreate** are at
`0001_baseline.sql:221-223`: `idx_index_jobs_status`, `idx_index_jobs_file_id`,
`idx_index_jobs_source_id`. Miss one and the P-02 count query loses its index.

On a catalog created after `c54e89d` the rebuild is a behavioural no-op — that is
fine and expected; do not try to detect and skip it.

**Step 2 — restore `0001_baseline.sql` to its 0.16.0 text**, after step 1 exists.
Order matters: restoring first would break fresh installs, which would then get
the narrow CHECK and no repair.

**Step 3 — delete the false claim in `0003`.** Already done by `7a9605c`; verify
and move on. Do not re-edit that file — see §2.

## 4. §6 — the downgrade guard

Move the version check from `run_check` into `Catalog::from_connection`, refusing
with a typed error naming both the stored and supported versions.

**The inversion worth stating**: `--check`, the headless diagnostic, *does* guard
this today. The GUI does not. So the tool that exists to find problems catches it
and the application that would suffer from it does not.

`from_connection` is also used by `open_in_memory` for tests, where the check is a
no-op. Note that as a choice rather than discovering it.

## 5. §9's remaining open questions — answers, and one still yours

**Q1, how CI determines the last tag.** `git describe --tags --abbrev=0` needs
`fetch-depth: 0`; the alternative is pinning a tag in the script and bumping at
release. **Take the first and set `fetch-depth: 0` on that job**, with a comment
saying why — a shallow checkout makes it fail confusingly, and the failure mode
of the second (a stale pin nobody bumps) is silent, which is worse.

**Q2, whether other migrations were edited.** Answered in §1. Two, not one.

**Q3, `from_connection` vs `open`.** Implementer's call; §4 above.

## 6. Definition of done

1. `0007` exists, rebuilds `index_jobs` with the full CHECK, recreates all three
   indexes, and a catalog built from the **0.16.0 tag** accepts
   `status='paused'` after upgrading — **observed rejecting it first**.
2. On that same upgraded catalog, toggling background indexing actually pauses
   indexing, observable as jobs ceasing to dispatch.
3. `git diff 0.16.0 HEAD -- crates/data/db/migrations/0001_baseline.sql` is empty.
4. A catalog stamped one version above `latest_version()` is refused by the
   application, with an error naming both versions; `--check` still reports it.
5. The gate rejects an edit to a released migration and accepts a new one, both
   demonstrated on a scratch branch rather than by inspection, and its self-test
   fails when the check is gutted.
6. The allowlist holds exactly one entry, `0003`, with its reason inline.

## 7. Stop conditions

- The `0007` rebuild loses rows, or an index, on a real 0.16.0-built catalog.
- The gate cannot enforce shrink-only cheaply in shell. **An honest note saying
  it is unenforced beats an approximation** — say so and I will scope it.
- §1's sweep result disagrees with what you measure. Re-run it with the
  self-check; if it still disagrees, my numbers are wrong and I want to know
  before the allowlist is written around them.
- Restoring `0001` changes any test's expected schema. It should not — the
  repair migration supplies what the restore removes — and if it does, step 1 is
  incomplete.
