# Closure Record — RFC-003: Source Registration and File Access Boundary

**RFC:** [003](../done/003-source-registration-and-file-access-boundary.md)
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** the original source and path-guard work (v0.1.0), Task 105
(only a folder can be added; the typed path), Task 109 (Amendment 1, §10a),
Task 110 (§10.2: the question before saving). None of Tasks 105, 109 or 110 is
git-tracked (RFC-063 §5): they are `.git-exclude/tasks/dev-team/…`, and the
review requests are `.git-exclude/review-request/283`, `287`, `288`.
**Transcribed, not re-derived**, from what was run on 2026-09-24. §13's list
was renumbered `1.`-style in the same change so the lifecycle gate reads it.

---

## §13 acceptance criteria

### 1. User can add persistent folder source.

→ what was run: `a_typed_path_is_added_and_no_picker_opens`
(`crates/app/src/router/tests.rs`, through `route`),
`adding_an_already_registered_folder_inserts_nothing_and_returns_the_existing_source`
(`crates/app/src/bootstrap/tests/task047_duplicate_source.rs`).
→ what was observed: a real directory is added as a `persistent` `directory`
source, its scan is queued, the card appears; a second add of the same
folder, written three ways, adds nothing.
→ where verified: `cargo test -p orbok --bin orbok router::tests bootstrap::tests`.

### 2. User can add temporary file source.

→ **dropped by owner decision, RFC-003 §10a (Amendment 1, Task 109).** A
source is a folder.
→ what was run: `add_source_refuses_a_file_and_creates_no_row` and
`a_typed_file_path_is_refused_and_creates_no_row`
(`bootstrap/tests/task105_only_folders.rs`, `router/tests.rs`).
→ what was observed: a file path is refused with the ordinary failure notice
and no `sources` row is created.

### 3. Backend canonicalizes paths.

→ what was run: the same-folder-three-ways test in criterion 1
(`folder`, `folder/`, `folder/.`), and `rejects_dot_dot_traversal`
(`crates/data/fs/src/tests/path_guard.rs`).
→ what was observed: every spelling resolves to one registered source;
a `..` traversal is rejected.

### 4. Backend rejects file reads outside active sources.

→ what was run: `rejects_path_outside_sources` and `rejects_symlink_escape`
(`crates/data/fs/src/tests/path_guard.rs`).
→ what was observed: `PathGuard::validate` refuses a path outside every
active source.
→ where verified: `cargo test -p orbok-fs path_guard`.

### 5. Hidden files are excluded by default.

→ what was run: `hidden_file_excluded_by_default`
(`path_guard.rs`) and `hidden_and_excluded_components_skipped`
(`crates/data/fs/src/tests/scanner.rs`).
→ what was observed: a dot-file and a dot-directory are skipped. `add_source`
stores `HiddenFilePolicy::Exclude` for every folder and there is no control
to change it (§10a). **Limit, observed in Task 110 §4:** a *source root that is
itself a listed directory* (`~/.ssh`) is not protected by this rule for its
non-hidden children (`notes.md` was `discovered`; the extension-less `id_rsa`
was `unsupported`, so its contents are never read).

### 6. Symlinks are ignored by default.

→ what was run: `symlinks_ignored_by_default`
(`scanner.rs`), `ignore_policy_blocks_internal_symlink` and
`ignore_policy_blocks_internal_symlink_via_symlinked_ancestor`
(`path_guard.rs`).
→ what was observed: a symlink, inside or outside the source, is not followed;
`add_source` stores `SymlinkPolicy::Ignore` for every folder.

### 7. Sensitive directory warning is shown.

→ what was run: `sensitive_paths_warn` (`crates/data/fs/src/tests/path_guard.rs`),
`every_way_of_adding_a_private_folder_asks_first_and_saves_nothing`,
`cancelling_the_private_folder_question_changes_nothing`,
`add_anyway_adds_and_prepares_the_folder_without_the_old_notice`,
`ordinary_and_already_added_folders_are_not_asked_about`
(`router/tests.rs`), and the dialog tests
(`crates/ui/src/tests/task110_private_folder_question.rs`).
→ what was observed: for the picker, the typed path and the search-in-folder
picker, a private folder raises the question **before** a `sources` or
`index_jobs` row exists; Cancel saves nothing, raises no notice and keeps the
typed text and the pending query; "Add anyway" adds and prepares it. The
dialog's exact copy, in both locales, is asserted; Escape cancels and Enter
confirms only while the dialog is visible. Three mutations (the check moved
after `add_source`; the search path skipping it; Enter confirming a hidden
dialog) each failed the tests above. §10.2's "add with exclusions" is dropped
(§10a); the remaining actions are Cancel and Add anyway.
→ where verified: `cargo test -p orbok --bin orbok router::tests` and
`cargo test -p orbok-ui task110`.

### 8. Source removal does not delete source files.

→ what was run: `removing_a_folder_never_deletes_its_files`
(`crates/app/src/bootstrap/tests/task110_rfc003_removal.rs`).
→ what was observed: after `remove_source` the registration is gone and the
file on disk exists with its content unchanged.

### 9. Source status supports active, paused, missing, permission denied, removed.

→ what was run: `searchable_status_sql_matches_the_enum`
(`crates/core/src/tests.rs`) and the schema's `CHECK` on `sources.status`
(`0001_baseline.sql`).
→ what was observed: all five values exist and are handled. (No screen sets
`paused`; that is a separate finding, see Task 107.)

### 10. Tests cover path traversal and symlink escape attempts.

→ what was run: `rejects_dot_dot_traversal`, `rejects_symlink_escape`
(`path_guard.rs`).
→ what was observed: both pass, and were written for exactly these attempts.

---

## Criteria not met, and why RFC-003 closes anyway

- **Criterion 2 (temporary file source)** is dropped by owner decision
  (§10a, Amendment 1). The RFC's own text says so.
- **§10.1's controls** (hidden-file and symlink policy, index mode,
  include/exclude rules) are dropped as user controls by the same amendment;
  their fixed values are criteria 5 and 6.
