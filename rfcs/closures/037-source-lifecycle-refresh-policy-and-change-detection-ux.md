# Closure Record — RFC-037: Source Lifecycle, Refresh Policy, and Change Detection UX

**RFC:** [037](../accepted/037-source-lifecycle-refresh-policy-and-change-detection-ux.md)
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** [Task 035](../../.git-exclude/tasks/dev-team/035-wire-rfc037-source-refresh.md),
verified in [Review 200](../../.git-exclude/reviewed/200-task035-wire-rfc037-source-refresh-review.md).
**Transcribed, not re-derived**, from Task 035 §5's own re-phrasing of the
criteria below into the falsifiable form RFC-058 §5 / RFC-063 §6.1 require,
and from [Review Request 200](../../.git-exclude/review-request/200-task035-wire-rfc037-source-refresh.md)'s
own §5/§7 observations.

---

## §21 acceptance criteria

### 1. Source states are explicit.

→ what was run: code inspection of `orbok_fs::source_lifecycle::SourceState`
(7-variant, RFC-037's own vocabulary) and `orbok_core::SourceStatus`
(5-variant, catalog-persisted — see "criteria not met" below for why these
differ) plus the `SourceState*` i18n label keys in `crates/ui/src/i18n.rs`.
→ what was observed: the module existed, fully built, since before Task
035, but was reachable from exactly one place (`lib.rs`'s `pub mod` line) —
states were explicit but unused. Task 035 made them load-bearing: `sources_view`
now derives its rendered label from `SourceStatus` for every card, not a
hardcoded string.
→ where verified: `sources_view_shows_folder_not_found_detail_copy` and
`sources_view_renders_both_states` (`crates/ui/src/tests/smoke_views.rs`),
commit `5b3db48`, CI run 33736428344.

### 2. File states are explicit.

→ what was run: code inspection of `orbok_core::FileStatus` (8 variants:
`Discovered`/`Indexed`/`Stale`/`Missing`/`Deleted`/`PermissionDenied`/
`Unsupported`/`Failed`).
→ what was observed: pre-existing, unchanged by Task 035 — the scanner
(`crates/data/fs/src/scanner.rs`) already assigned these on every scan.
Task 035 gave the scanner something to run against for the first time
outside of add-source (see criterion 3), which is what makes this state
machine actually exercised in the running application.
→ where verified: `crates/data/fs/src/tests/scanner.rs` (pre-existing
suite, 41/41 passing after Task 035's changes).

### 3. Startup check exists.

→ what was run: `restarting_orbok_picks_up_a_file_edited_while_closed`
(`crates/app/src/wired_application_tests.rs`) — registers a source, indexes
it, "closes" (drops every handle), edits the file on disk with nothing
running, then calls `bootstrap::load_initial_state` (the real startup
entry point `main.rs` calls) and searches.
→ what was observed: PASS — the revised content is found after restart.
Confirmed failing before `load_initial_state` was taught to enqueue a
startup scan (RFC-058 §6's own row 1).
→ where verified: commit `60f8602`, CI run 33736428344 (all seven jobs,
including the `cross` job's three-platform matrix — `wired_application_tests`
is declared in `main.rs`, so it runs inside `--bin orbok`, which
`ci.yml:252` runs on Linux, macOS, and Windows).

### 4. Manual refresh exists.

→ what was run: `manual_refresh_picks_up_a_file_added_while_running`
(same file) — a source stays registered and "running", a new file appears
on disk, `bootstrap::check_and_refresh_source` (the function the
`[Check again]`/`[Prepare again]` button and `Ctrl/Cmd+R` both call) is
invoked, then searched. Keyboard reachability proven separately:
`refresh_selected_source_by_keyboard` /
`refresh_selected_source_by_keyboard_fails_without_the_binding`
(`crates/ui/src/tests/keyboard_reachability.rs`).
→ what was observed: PASS on all three — the new file is found; the
keyboard binding fires the correct message when a source is selected on
the Sources view and does not fire otherwise (wrong view, no selection).
→ where verified: commit `60f8602`, CI run 33736428344.

### 5. Missing folders are recoverable.

→ what was run:
`a_renamed_or_unmounted_folder_is_marked_missing_at_startup_and_nothing_is_deleted`
(`wired_application_tests.rs`) — registers a source, indexes it, deletes
the folder itself (`remove_dir_all`, standing in for a rename or an
unmount), then restarts.
→ what was observed: the source is listed with `SourceStatus::Missing`
(RFC-037's `FolderNotFound`), not silently dropped or errored; the source
row and its file row both survive (`COUNT(*) = 1` each, asserted
directly, not assumed); §17.3's explanatory copy
("This can happen if a drive is disconnected or the folder was moved.")
renders on the card (fixed in commit `5b3db48` after this task's own
re-reading of §17 found the gap).
→ where verified: commits `60f8602` and `5b3db48`, CI run 33736428344.

### 6. Removed folders do not delete source files.

→ what was run: code inspection of
`SourceRepository::delete_with_all_data` (`crates/data/db/src/repo/sources.rs`)
— the function `remove_source` (the `[Remove from orbok]` button) calls.
→ what was observed: a single `DELETE FROM sources` against the catalog
connection, cascading to `files`/`chunks`/etc. via `ON DELETE CASCADE`
foreign keys. The function takes only `&Catalog`; it has no filesystem
access of any kind, so it cannot touch a file on disk by construction, not
merely by having not been observed to. Pre-existing, unchanged by Task 035.
→ where verified: `source_delete_cascades_to_files`
(`crates/data/db/src/tests.rs`, pre-existing), confirming the catalog-side
cascade; the filesystem-inertness claim above is a structural property of
the function's signature, not a test result.

### 7. Change detection marks files as needing update.

→ what was run: code inspection of `Scanner::process_file`'s
metadata/hash comparison (`crates/data/fs/src/scanner.rs`) — pre-existing,
unchanged by Task 035.
→ what was observed: an indexed file whose content hash changed is marked
`FileStatus::Stale` and a new `Extract` job is queued. What Task 035 added
is reachability: before this task, this logic only ever ran once, at
add-source time — it now runs on every startup and every manual refresh,
so a file edited after the initial index is actually detected in practice,
not just in the unit suite.
→ where verified: `modified_file_marked_stale`
(`crates/data/fs/src/tests/scanner.rs`, pre-existing) plus
`restarting_orbok_picks_up_a_file_edited_while_closed`
(`wired_application_tests.rs`, new) as the end-to-end proof that this path
is actually reached from the application's real entry points.

### 8. Search can still use prepared data while refresh is pending.

→ what was run: every `wired_application_tests.rs` test searches
immediately after enqueueing a scan/refresh, without waiting for anything
beyond the specific job whose result it asserts on;
`startup_rescan_extracts_and_chunks_on_battery_but_defers_embedding`
(`crates/app/src/scheduler_host/tests.rs`) additionally confirms keyword
search keeps working via already-indexed content while an embedding job
for other content sits deferred, indefinitely, behind the battery policy.
→ what was observed: search never blocks on a queued/running job — it
reads catalog state directly (`bootstrap::run_search`), which is
independent of `index_jobs` by construction. RFC-037's own §10.1 states
"needs-update state does not block existing search"; nothing in Task 035
changed that invariant, and every new test's search call is itself
evidence it held throughout.
→ where verified: all `wired_application_tests.rs` and
`scheduler_host/tests.rs` runs, commit `60f8602`, CI run 33736428344.

### 9. Live watcher is not required for initial stability.

→ what was run: `grep -r notify Cargo.toml` across the workspace; RFC-037
§10.3 read in full; Task 035 §8's stop conditions checked against the
finished work.
→ what was observed: `notify` appears in no manifest. §10.3 (automatic/
live refresh) stays deferred, exactly as before this task — see "criteria
not met" below. Startup check + manual refresh (criteria 3, 4) satisfy
this criterion's actual requirement without a watcher.
→ where verified: Task 035's own submission (Review Request 200 §8),
confirmed in Review 200's disposition — no stop condition triggered.

### 10. User-facing labels avoid technical terms.

→ what was run:
`grep -n "SourceState\|SourceAction" crates/ui/src/i18n/en.rs crates/ui/src/i18n/ja.rs`;
`sources_view_shows_folder_not_found_detail_copy` and
`sources_view_renders_both_states`
(`crates/ui/src/tests/smoke_views.rs`); the i18n literal-copy gate
(`scripts/check-i18n-literals.sh`).
→ what was observed: the `SourceState*`/`SourceAction*` keys use RFC-037's
own plain-language copy ("Ready", "Folder not found", "Check again",
"Prepare again"), fully translated in both locales — existing since before
this task, but the one call site that rendered source status text used a
different, older set of keys (`SourcesStatusActive`/`SourcesStatusPaused`/
`SourcesStatusMissing`) with non-RFC-037 wording ("Active"/"Paused"/
"Missing"). Task 035 switched the call site to the correct RFC-037 keys;
the older keys are left defined but unused, matching this project's
existing `BadgeFused` precedent for a superseded-but-still-valid key.
→ where verified: commit `60f8602`; `check-i18n-literals.sh` passing (no
unrouted literal source text in the Sources view), CI run 33736428344.

---

## Criteria not met, and why RFC-037 closes anyway

- **§10.3, automatic refresh (live watching), remains deferred.** Task 035
  §8's stop conditions explicitly named this: nothing in this work required
  `notify` or a filesystem watcher, and none was added. This was RFC-037's
  own explicit deferral going in (§10.3 is marked "deferred," not
  "required," in the RFC itself) — criterion 9 above is the one that
  actually governs, and it is met. Automatic refresh is future work, not a
  gap in what shipped.

- **§19's data model specifies `SourceRecord.state: SourceState` — the
  7-variant enum — as a persisted field. The implementation persists 5
  variants** (`orbok_core::SourceStatus`, matching the `sources.status`
  catalog CHECK constraint) **and derives `Preparing`/`NeedsUpdate` at
  render time** from queued-job and stale-file counts the catalog already
  holds, rather than persisting them. This is a deliberate deviation, not
  an oversight: persisting a queue-derived fact is denormalization that can
  go stale, a worse failure mode than deriving it fresh on every render.
  Review 200 §2.2 confirmed the engineering call and required a recording
  amendment rather than a design change — see the amendment note added to
  RFC-037 §19 itself in this same closure pass, in the shape of RFC-057's
  Amendment 1.

## Known follow-up (not a closure blocker)

**Manual refresh does not de-duplicate against an in-flight scan for the
same source** (Task 035 Review Request 200 §7 Q2; Review 200 §3 confirmed
this is a real gap, not an RFC-037 §14 "Change Storms" violation — §14
governs *file*-change debouncing for the still-deferred automatic refresh,
not repeated manual invocation). `IndexJobRepository::enqueue` performs no
existence check, so N clicks queue N full directory walks; each is cheap
(RFC-004's unchanged-file fast path) but N is unbounded. Review 200's
suggested shape: guard at enqueue time (skip when a `queued`/`running`
`Scan` already exists for that source) rather than disabling the button,
since a guard at enqueue also covers the startup path. Left for whoever
picks it up next; not required for this closure.
