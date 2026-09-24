# Closure Record — RFC-064: What a Folder Covers

**RFC:** [064](../done/064-what-a-folder-covers.md)
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** Task 113 (every file belongs to one folder: §3.3, criteria
5, 6 and 8) and Task 114 (a folder can leave out its subfolders: §3.1, §3.2,
§3.4, §3.5 and criteria 1–4, 7 and 9). Neither task is git-tracked (RFC-063
§5): they are `.git-exclude/tasks/dev-team/113-…` and `114-…`, and the review
requests are `.git-exclude/review-request/291` and `292`.
**Transcribed, not re-derived**, from what was run on 2026-09-25.

---

## §5 acceptance criteria

### 1. A card shows what its folder covers, and changes it.

→ what was run: `the_card_shows_the_choice_and_a_button_for_the_other`
(`crates/ui/src/tests/task114_folder_covers.rs`), `the_card_says_what_the_folder_covers`
(`crates/app/src/bootstrap/tests/task114_a_folder_can_leave_out_its_subfolders.rs`),
`widening_over_an_added_subfolder_combines_them_and_says_so` and
`asking_fetches_the_count_and_confirming_dispatches_the_request`
(`crates/app/src/router/tests.rs`).
→ what was observed: the rendered card shows the current choice ("This folder
and subfolders" / "This folder only", both locales) and a button labelled with
the other; the button sends `AskNarrowFolder` for a folder with subfolders and
`WidenFolder` for one without; the card is read back from the catalog record's
own `covers_subfolders`, and the router re-reads the cards after either change.
→ where verified: `cargo test -p orbok-ui task114`,
`cargo test -p orbok --bin orbok task114 router::tests`.

### 2. **This folder only** prepares only the files directly in the folder.

→ what was run: `this_folder_only_prepares_only_the_top_level`
(`bootstrap/tests/task114_…rs`).
→ what was observed: a folder with `x.md`, `sub/y.md` and `sub/deep/z.md`, set
to only, is scanned: the scan reads one file (`seen_files == 1`, so the
scanner does not walk `sub/`), `x.md` has a row and `sub/` has none. The
control, the same tree covering its subfolders, gets all three rows. The
insert of a file below the top level also checks the folder's setting in the
same statement (`a_scan_that_started_before_narrowing_cannot_write_below_the_top_level`),
so a scan that began before a narrowing writes nothing afterwards.
→ mutation: the scanner ignoring the setting fails the first test; removing
the insert's check fails the second.

### 3. Narrowing asks first, with a counted line. Afterwards, no chunk, index row, cache entry or catalog row remains for a file that fell out of the folder, and none is shown as File not found.

→ what was run: `the_question_is_worded_as_approved_with_a_count_only_when_there_is_one`,
`cancel_and_escape_change_nothing`, `enter_confirms_only_while_the_question_is_visible`,
`confirming_requests_the_narrowing_and_changes_nothing_yet`
(`crates/ui/src/tests/task114_folder_covers.rs`), the generic Task 069 tests
(extended with the question), `narrowing_erases_everything_prepared_below_the_top_level`,
`narrowing_while_subfolder_files_are_queued_cancels_their_jobs`,
`narrowing_mid_preparation_leaves_no_job_and_no_row`
(`crates/app/src/scheduler_host/tests.rs`), and
`the_confirmed_narrowing_runs_off_the_update_thread`,
`a_failed_narrowing_says_so_and_retrying_asks_again` (`router/tests.rs`).
→ what was observed: the dialog shows the approved title, body, confirm and
Cancel in both locales, names the folder, and shows the counted line with a
count (singular and plural) and never without one or with a zero; Cancel and
Escape change nothing; Enter confirms only the question on screen. After a
confirmed narrowing of a fully prepared tree (embeddings included): the
below-top-level files have no `files` row, no chunks, no `embeddings`, no
extraction-cache entry; `count(chunk_fts) == count(keyword_index_records)`;
nothing is `missing` or `deleted`; startup queues nothing for them; search
finds only the top-level file. Queued jobs for the files that left are
cancelled with their rows (41 queued → 1); the erasure is a dispatched task,
nothing is erased when `route` returns; a failure raises "Cleanup didn't
finish" whose retry re-opens the question, and the cards are re-read either
way.
→ mutations: marking files missing instead of erasing, skipping the FTS
delete, erasing only the indexed files (leaving queued work), and skipping the
cache eviction each failed one or more of these tests.

### 4. Widening prepares the subfolders without asking.

→ what was run: `widening_prepares_the_subfolders`
(`scheduler_host/tests.rs`), `widening_over_an_added_subfolder_combines_them`
(`bootstrap/tests/task114_…rs`), `widening_over_an_added_subfolder_combines_them_and_says_so`
(`router/tests.rs`).
→ what was observed: a folder set to only and prepared (`x.md`) is widened:
no question is opened (`route` returns no task), a scan is queued, and after
the host drains `y.md` is prepared and found. Added folders inside it become
part of it in the same transaction as the setting, with Task 113's notice.

### 5. No file is ever registered under two folders, whichever order the user adds, widens or searches in (§3.3, all four rows).

→ what was run: for the four rows of §3.3 — adding inside a covering folder:
`a_folder_inside_an_added_folder_registers_nothing`; adding above added
folders: `a_folder_above_added_folders_takes_their_files_with_it`,
`a_chain_combines_into_the_top_folder`; a subfolder of a this-folder-only
folder: `a_subfolder_of_a_this_folder_only_folder_is_added`; searching in a
subfolder: `search_in_a_subfolder_registers_nothing`; widening over added
folders: `widening_over_an_added_subfolder_combines_them`; and the existing
profile: `overlapping_folders_are_combined_once`
(`bootstrap/tests/task113_one_folder_per_file.rs`, `task114_…rs`,
`router/tests.rs`).
→ what was observed: after each, every canonical path has exactly one `files`
row and the erasure invariant holds
(`a_folder_added_above_a_prepared_one_keeps_what_was_prepared`,
`combining_an_overlapping_pair_leaves_one_prepared_copy_of_each_file`).
Only a folder that covers its subfolders covers what is below it; a folder set
to only, and the same folder in the startup combine, are the same folder as
themselves and nothing else
(`the_startup_combine_leaves_a_subfolder_of_a_this_folder_only_folder_alone`).
→ mutations (Task 113): a string-prefix cover check, forgetting to move
`files` or `index_jobs`, search-in-folder registering again, and a
non-idempotent combine each failed the tests above.

### 6. Searching in a subfolder of an added folder registers nothing and finds only files in that subfolder.

→ what was run: `search_in_a_subfolder_registers_nothing` (`router/tests.rs`),
`a_chosen_subfolder_is_found_inside_the_added_folder`
(`task113_one_folder_per_file.rs`), `a_search_limited_to_a_subfolder_finds_only_files_under_it`
(`scheduler_host/tests.rs`).
→ what was observed: choosing `a/b` with `a` added registers nothing; the
location is `a` limited to `a/b`, named `b`; the query finds `y.md` and `z.md`
and not the sibling `b2/w.md` (the component rule, in SQL), and "only" counts
from the subfolder.

### 7. A search in a **this folder only** folder offers no scope toggle.

→ what was run: `a_this_folder_only_folder_offers_no_scope_toggle`,
`a_stale_remembered_subfolders_scope_is_shown_and_stored_as_only`,
`narrowing_clears_a_location_inside_the_folder_and_keeps_the_query`
(`crates/ui/src/tests/task114_folder_covers.rs`).
→ what was observed: the search row for a this-folder-only folder shows the
"only" chip and no toggle in either locale; a folder with subfolders keeps
the toggle; a remembered "and subfolders" for a folder that has since been
narrowed is shown and **stored** as "only" however the card arrives (reload,
refresh or selection); a location limited to a subfolder of a narrowed folder
is cleared and the query stays.
→ mutation: showing the toggle for a this-folder-only folder fails the first
test; not normalising fails the second and third.

### 8. Overlapping folders in an existing profile are combined once at startup, with the notice.

→ what was run: `overlapping_folders_are_combined_once`,
`the_first_start_says_folders_were_combined_and_the_second_is_silent`
(`task113_one_folder_per_file.rs`), `combining_an_overlapping_pair_leaves_one_prepared_copy_of_each_file`
(`scheduler_host/tests.rs`), and the migration test
`an_existing_profile_opens_with_every_folder_covering_its_subfolders`
(`crates/data/db/src/tests/task114_source_covers_subfolders_migration.rs`).
→ what was observed: a profile holding `a`, `a/b`, `a/b/c` opens with one
folder, "Folders combined" shown; the second start says nothing; each file has
one row, the prepared copy kept, the index consistent. An existing version-9
profile opens with every folder set to cover its subfolders.
→ mutations: a combine that reports a group when it took nothing, and one that
leaves the taken folders behind, each failed these tests.

### 9. `SourcesRecursiveHint` is gone; the glossary and the unused-key test (Task 109) stay green.

→ what was run: `the_recursive_hint_is_gone`
(`crates/ui/src/tests/task114_folder_covers.rs`), the glossary tests
(`crates/ui/src/tests/glossary.rs`, with the new formatters in its scan) and
`task109_every_key_is_shown`.
→ what was observed: the key, its two strings and its line on the Folders page
are deleted (the enum has no such variant); the Folders page renders neither
sentence; the glossary and the unused-key test pass with the ceiling unchanged.

---

## Criteria not met, and why RFC-064 closes anyway

None.

**Not exercised through the window:** the search-in-a-subfolder flow, which
starts from the native folder picker. The same handler runs in the router
tests (criteria 6 and 7) and the query in the scheduler-host test.
