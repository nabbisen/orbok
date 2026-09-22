# Closure Record — RFC-038: Result Freshness, Trust Badges, and Recovery Actions

**RFC:** [038](../done/038-result-freshness-trust-badges-and-recovery-actions.md)
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** `rfcs/handoffs/HANDOFF-038-rendering-trust-and-recovery.md`
(Slices 1–2, wiring `result_trust_badge` and the recovery-action row into
the result card) and Task 082 (`.git-exclude/tasks/dev-team/082-every-result-offers-something-to-do.md`,
lifting HANDOFF-038 §3's hold on `OpenAnyway`/`ShowInFolder`). Commits:
`9e499f5` (HANDOFF-038), `b6ac890` (Task 082).
**Transcribed, not re-derived**, from what was run in this session.

---

## §16 acceptance criteria

### 1. Result trust states are explicit.

→ `ResultTrustState` (`crates/search/engine/src/result_trust.rs`): a closed
six-variant enum (`Ready`, `NeedsUpdate`, `FileNotFound`,
`StillBeingPrepared`, `PartlyPrepared`, `CannotOpen`), derived from the
catalog's `file_status` column by `trust_from_file_status`. No stringly-typed
state reaches the UI.
→ where verified: `cargo test -p orbok-search`.

### 2. Default UI shows trust badges only when useful.

→ `ResultTrustState::show_badge_by_default` returns `false` only for
`Ready`; `trust_recovery` (`crates/ui/src/views.rs`) returns `None` (renders
nothing) for a `Ready` result.
→ what was run: `a_non_ready_result_shows_its_trust_label_and_a_ready_one_shows_none`
(`crates/ui/src/tests/handoff038_trust_display.rs`).
→ where verified: `cargo test -p orbok-ui --lib handoff038_trust_display`.

### 3. Warnings from extraction are represented in result trust.

→ `ResultWarningSummary::from_extract_warning` maps `ExtractWarning` into
the five UI-facing summaries; `SearchResultTrust::from_catalog` folds them
into `PartlyPrepared` when any degrading warning is present.
→ what was run: `a_real_file_with_an_extraction_warning_is_partly_prepared_and_still_found`
(`crates/app/src/wired_application_tests.rs`), through the real extraction
pipeline against a real scanned-PDF fixture.
→ where verified: `cargo test -p orbok --bin orbok a_real_file_with_an_extraction_warning`.

### 4. Missing files do not appear as normal ready results.

→ `snippet.rs`'s `effective_status`: a result's trust is computed from the
live `Path::exists()` check when the catalog is stale, not from the
catalog's `file_status` alone -- a deleted file reads `FileNotFound` even
before a refresh runs.
→ what was run: `a_result_for_a_file_deleted_from_disk_is_not_labelled_ready`
and `deleting_a_file_marks_it_missing_and_removes_it_from_search_results`
(`crates/app/src/wired_application_tests.rs`).
→ what was observed: PASS. The first deliberately does not refresh the
source first, closing the exact gap the RFC's return-to-`accepted/` note
(RFC-063 §7, 2026-09-02) named: `bootstrap/search.rs` no longer hardcodes
`ResultTrustDisplay::default()`.
→ where verified: `cargo test -p orbok --bin orbok deleting_a_file_marks_it_missing`;
`cargo test -p orbok --bin orbok a_result_for_a_file_deleted_from_disk`.

### 5. Changed files show Needs update.

→ `"stale"` catalog status maps to `ResultTrustState::NeedsUpdate` with
`[PrepareAgain, OpenAnyway]`.
→ what was run: `a_changed_file_needs_an_update_until_prepare_again_makes_it_ready`
(same file), through the real refresh path.
→ where verified: `cargo test -p orbok --bin orbok a_changed_file_needs_an_update`.

### 6. Partly prepared files are honest but still searchable.

→ `PartlyPrepared`'s chunks are never removed; only the file's status
changes. `ChunkRepository::deactivate_for_missing_files`'s own doc records
that a status flip has no effect on searchability by itself.
→ what was run: `a_real_file_with_an_extraction_warning_is_partly_prepared_and_still_found`
(same file as criterion 3 -- its own name states both halves of this
criterion).
→ where verified: same command as criterion 3.

### 7. Recovery actions are available.

→ **The criterion this closure record turns true.** Before Task 082,
`recovery_label` (`views.rs`) returned `None` for `OpenAnyway` and
`ShowInFolder` (HANDOFF-038 §3's deliberate hold, reasoning orbok had no
way to open a file at all). Task 041 gave it one; `launch_request`
(`crates/app/src/result_launch.rs`) already mapped both actions through
that same catalog-checked path before this task touched anything. Task 082
changed only the rendering: `recovery_label` now returns
`TrustActionOpenAnyway`/`TrustActionShowInFolder`.
→ what was run: `recovery_buttons_match_the_state_and_send_their_action`
(rewritten this task -- previously asserted the two actions **never**
render; now asserts every state's rendered buttons match
`trust.recovery_actions` exactly, in order, for all six actions),
`no_state_with_actions_is_button_less` (new, exhaustive: every non-Ready
state with any action renders at least one button; `StillBeingPrepared`,
the one state with none, is the named exception),
`open_anyway_and_show_in_folder_reach_the_launcher_through_the_same_path_as_open_and_reveal`
and `show_in_folder_on_an_unreadable_path_is_not_allowed_not_not_found`
(new, `crates/app/src/result_launch/tests.rs`).
→ what was observed: PASS, all four. Mutations, each restored
byte-identical (`cmp`): `recovery_label` returning `None` for `OpenAnyway`
again fails `recovery_buttons_match_the_state_and_send_their_action` and
`no_state_with_actions_is_button_less`; `launch_request` mapping
`ShowInFolder` to `LaunchAction::Open` instead of `Reveal` fails
`open_anyway_and_show_in_folder_reach_the_launcher_through_the_same_path_as_open_and_reveal`
and the pre-existing `recovery_actions_share_the_open_and_reveal_path`.
→ **Task 082 §1.4's specific question** -- what a `CannotOpen` result's
*Show in folder* does when the folder is unreadable -- answered by
`show_in_folder_on_an_unreadable_path_is_not_allowed_not_not_found`:
validation runs identically before either launcher method is chosen (only
the final `match action` differs, after validation), so a permission-denied
folder reaches `LaunchFailure::NotAllowed` →
`UserNotice::FileNotAllowed`, never a false `NotFound`, for `Reveal`
exactly as it already did for `Open`.
→ where verified: `cargo test -p orbok-ui --lib handoff038_trust_display`;
`cargo test -p orbok --bin orbok result_launch`.

### 8. Status is not communicated by color alone.

→ Every trust badge carries an icon and a text label
(`components::trust_tone`/`tone_icon`); `every_trust_badge_has_a_distinct_label_and_an_icon`
(`handoff038_trust_display.rs`, unchanged by this task) proves the five
non-Ready labels are pairwise distinct in both locales.
→ **Extended this task**, per Review 254 §5 follow-up 1:
`cvd_greyscale_trust_status_distinguishable` (new,
`crates/ui/src/tests/a11y.rs`). `trust_tone` is not injective --
`NeedsUpdate`/`PartlyPrepared` share Warning, `FileNotFound`/`CannotOpen`
share Danger -- so those two pairs share both tone and icon under a
greyscale or colour-blind collapse, and the label is the only channel
left. The test asserts that explicitly (not just "icon or label differs"
generically): for every colliding pair, the label must differ, in both
locales.
→ what was observed: PASS.
→ where verified: `cargo test -p orbok-ui --lib a11y::cvd_greyscale_trust_status_distinguishable`.

### 9. Advanced view can show more detail.

→ `trust_detail_keys`/`trust_recovery` (`views.rs`): a result's detail
lines show when Advanced view is on, or when `ViewDetails` was pressed
(closed by default otherwise).
→ what was run: `view_details_shows_the_detail_and_advanced_view_shows_it_unasked`
(`handoff038_trust_display.rs`, unchanged by this task).
→ where verified: `cargo test -p orbok-ui --lib handoff038_trust_display::view_details_shows_the_detail`.

### 10. Copy avoids technical terms.

→ Every trust label and recovery-action label routes through `MessageKey`
and owner-approved copy (`i18n/en.rs`, `i18n/ja.rs`) -- "Needs update",
"Cannot open", "Open file anyway", never a raw enum name. Covered by the
project-wide plain-language guard in `rfc041_search.rs`/`rfc041_search_state.rs`,
which the six trust-action `MessageKey`s (including the two Task 082
newly renders) pass through like every other UI string.
→ where verified: `cargo test -p orbok-ui --lib rfc041`.

---

## Every criterion evidenced -- why this closes now, not before

The RFC's return-to-`accepted/` note (RFC-063 §7, 2026-09-02) named
criteria 4, 5 and 7 false, pointing at `bootstrap/search.rs` hardcoding
`ResultTrustDisplay::default()`. That wiring landed with RFC-060's later
slices (`bootstrap/search.rs` now maps `r.trust.state` from the real
search result, not a constant) and HANDOFF-038's own slices 1–2, closing
4, 5, 6, 8, 9 before this task started. Criterion 7 alone remained false,
for the specific reason recorded in `rfcs/README.md`'s `accepted/` entry
and reported in Review Request 254 §4/§6: `recovery_label` deliberately
returned `None` for `OpenAnyway`/`ShowInFolder`. Task 082 is exactly that
fix, on the owner's approval (Review 254 §4–5, 2026-09-22) that the
original reason -- "orbok has no way to open a file at all" -- no longer
holds since Task 041.

## Not claimed in this record

Task 082 §2.2 asked for real-window screenshots (a `FileNotFound` row and
a `CannotOpen` row, narrow width, `Larger` text scale, both locales) as
part of its own definition of done. **These were attempted and not
obtained this session:** a scratch profile was built through the real
pipeline (`bootstrap::scan_and_index_source`, a genuinely deleted file for
`FileNotFound`, `FileRepository::set_status(PermissionDenied)` for
`CannotOpen` -- the state a real permission-denied scan leaves a file in
until the next rescan, since `snippet.rs`'s live `Path::exists()` check
cannot distinguish "briefly locked" from "gone" for a currently-unreadable
path), and the real `orbok` binary was launched against it under XWayland.
`niri msg action screenshot-window` + `wl-paste` reliably captured the
window's current state, confirming the process was alive and rendering --
but no input reached it: `xdotool click`/`key` (recomputed coordinates,
`windowfocus --sync`, the Ctrl+1 view shortcut) and niri's own
`focus-window`/`toggle-window-floating` IPC actions each returned success
but produced no observable change, and `niri msg -j windows` reported
`is_focused: false` for every window on the output throughout, including
after an explicit `focus-window`. This reproduced across every approach
tried; it is an environment input-routing limitation in this session, not
a rendering defect -- §16 criterion 8's actual property (label
disambiguates every tone collision) is proven above by
`cvd_greyscale_trust_status_distinguishable`, which needs no screenshot.
None of §16's ten criteria depend on the missing screenshots, so this
closure does not wait on them; Task 082's own review request names the gap
separately.

## Full gate suite, this implementation

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
--all-features -- -D warnings`, `cargo test --workspace --all-features`
(0 failures), every `scripts/check-*.sh` gate, `mdbook build docs`, and
`git diff --check` -- green, commit `b6ac890`.
