# RFC-003: Source Registration and File Access Boundary

**Project:** orbok  
**RFC:** 003  
**Title:** Source Registration and File Access Boundary  
**Status:** Implemented (unreleased)
**Target Milestone:** M2  
**Date:** 2026-06-06  

**Returned to `accepted/` 2026-09-24 (Task 109) and closed again 2026-09-24
(Task 110).** It carried `Implemented (v0.1.0)` while §10.2's warning **before
saving** was not built: orbok saved the folder, queued its files, and only then
showed a notice. Amendment 1 (§10a) dropped what the owner decided not to offer
and kept §10.2 without "add with exclusions"; Task 110 built it: a folder that
may contain private files is asked about before anything is saved, from the
Add folder picker, a typed path and the search-in-folder picker. The evidence
for every criterion is `rfcs/closures/003-source-registration-and-file-access-boundary.md`.

---

## 1. Summary

This RFC defines how `orbok` registers local files and folders as searchable sources and how the backend enforces a safe file-access boundary.

The central rule is:

> The frontend never receives unrestricted file-system access. The Rust backend reads only user-approved sources under explicit source policies.

---

## 2. Motivation

`orbok` is a local-first app, but local-first does not automatically mean safe. A local document search app can accidentally expose sensitive files, index secrets, follow symlinks outside intended folders, or allow a local web UI to read arbitrary paths.

Source registration and file access must therefore be explicit, auditable, and enforced by the backend.

---

## 3. Goals

- Allow users to register files and folders.
- Support persistent and temporary sources.
- Enforce allowlist-based file access.
- Canonicalize paths before access.
- Support hidden-file and symlink policies.
- Warn about sensitive directories.
- Prevent frontend-controlled arbitrary file reads.
- Provide clear UI/API behavior.

---

## 4. Non-Goals

- This RFC does not implement scanning itself.
- This RFC does not define extraction.
- This RFC does not implement permission elevation.
- This RFC does not implement multi-user ACLs.
- This RFC does not sandbox third-party parsers.

---

## 5. Source Types

## 5.1. Persistent Source

A persistent source is remembered across app restarts.

Examples:

- `~/Documents`
- `~/Projects`
- a stable client folder

Persistent sources are included in normal rescans.

## 5.2. Temporary Source

A temporary source is intended for one-off search.

Examples:

- a downloaded PDF;
- a folder dragged into the app for a single session;
- a report that should not remain searchable.

Temporary sources have retention policy:

```text
until_app_close
until_explicit_cleanup
for_n_days
```

---

## 6. Source Policy

Each source has:

```text
source_type
persistence_mode
canonical_path
display_path
index_mode
include_patterns
exclude_patterns
hidden_file_policy
symlink_policy
max_file_size_bytes
status
```

## 6.1. Hidden File Policy

Allowed values:

```text
exclude
include
warn
```

Default:

```text
exclude
```

## 6.2. Symlink Policy

Allowed values:

```text
ignore
follow_within_source
follow_all_with_warning
```

Default:

```text
ignore
```

Recommended v1 behavior:

- support `ignore`;
- support `follow_within_source`;
- defer `follow_all_with_warning` unless clearly needed.

## 6.3. Include/Exclude Patterns

**Superseded by Amendment 2 (§10b, Task 120).** The list of default excludes
this section once carried (`.git`, `node_modules`, `target`, `dist`, `build`,
`.cache`, `.venv`, `__pycache__`, "configurable") is replaced by a rule: what is
skipped is decided by what the platform hides and by evidence that a tool
generated a folder, is fixed, and is not configurable. `target`, `dist` and
`build` are ordinary words and are no longer skipped by name.

---

## 7. Sensitive Directory Warnings

The app should warn before indexing directories likely to contain secrets.

Initial warning targets:

```text
~/.ssh
~/.gnupg
~/.aws
~/.azure
~/.config
browser profile directories
password manager exports
system directories
```

The warning should say:

- source files will not be uploaded;
- but local indexes may contain derived data;
- indexing secrets is not recommended.

---

## 8. Backend Access Rule

Before any backend reads a file, it must verify:

1. path is canonicalized;
2. path belongs to an active source;
3. source policy permits file type;
4. hidden-file policy permits it;
5. symlink policy permits it;
6. file size is within limit;
7. file still exists and is readable.

The backend must not trust frontend-provided paths.

---

## 9. Source API

Conceptual API:

```text
GET    /api/sources
POST   /api/sources
GET    /api/sources/{source_id}
PATCH  /api/sources/{source_id}
DELETE /api/sources/{source_id}
POST   /api/sources/{source_id}/scan
POST   /api/sources/{source_id}/pause
POST   /api/sources/{source_id}/resume
```

## 9.1. Add Source Request

```json
{
  "path": "/home/user/Documents",
  "source_type": "directory",
  "persistence_mode": "persistent",
  "index_mode": "balanced",
  "hidden_file_policy": "exclude",
  "symlink_policy": "ignore",
  "include_patterns": ["*.md", "*.pdf", "*.txt"],
  "exclude_patterns": [".git", "node_modules", "target"],
  "max_file_size_bytes": 104857600
}
```

## 9.2. Add Source Response

```json
{
  "source_id": "src_...",
  "status": "active",
  "canonical_path": "/home/user/Documents",
  "warnings": []
}
```

## 9.3. Warning Response

```json
{
  "accepted": false,
  "warning": {
    "kind": "sensitive_directory",
    "message": "This folder may contain private credentials.",
    "recommended_action": "do_not_index"
  }
}
```

---

## 10. UI Requirements

## 10.1. Add Source Dialog

The dialog must expose:

- folder/file selector;
- persistent/temporary choice;
- index mode;
- hidden-file policy;
- symlink policy;
- include/exclude rules.

## 10.2. Sensitive Source Warning

If a risky source is selected, show a warning before saving.

Actions:

- cancel;
- add with exclusions;
- add anyway.

## 10.3. Remove Source Dialog

Removing a source must clarify:

- source files will not be deleted;
- source registration can be removed;
- rebuildable index data can optionally be removed;
- all orbok data for the source can optionally be removed.

---

## 10a. Amendment 1 (2026-09-24) — what orbok does not offer

Owner decision 2026-09-24: **fixed safe defaults; less to configure.** Task
109 (origin: Review Request 285 §3) records what §10.1 and §13 asked for and
orbok does not offer:

- **§10.1's single-file selector and the persistent/temporary choice are
  dropped**, and so is §13's "User can add temporary file source". A source
  is a folder.
- **§10.1's hidden-file policy, symlink policy, index mode and
  include/exclude rules are dropped as user controls.** The values are fixed:
  hidden files are excluded and symlinks are not followed (§13's two
  "by default" criteria hold as *fixed* behaviour). Where: `add_source`
  stores `HiddenFilePolicy::Exclude` and `SymlinkPolicy::Ignore`
  (`crates/app/src/bootstrap/sources.rs`); the scanner reads them
  (`crates/data/fs/src/scanner.rs`, `skip_component` and `symlink_allowed`).
  The columns stay in the schema (RFC-002).
- **§10.2 is kept, without "add with exclusions".** If a risky source is
  selected, orbok asks before saving: cancel, or add anyway. It is met by
  Task 110.

## 10b. Amendment 2 (2026-09-26) — what orbok skips

Task 120 (origin: the owner's question of 2026-09-25, and Review Request 288 §4:
on Windows, adding the user's profile folder prepared files from inside
`AppData`). It replaces §6.3's list of default excludes with a rule, and keeps
Amendment 1's decision that it is fixed (no setting, no per-folder option).

- **Hidden means what the platform means.** A file or folder inside an added
  folder is skipped when its name starts with `.` (every platform); on Windows
  when it has the Hidden or System attribute (this is what hides `AppData`); on
  macOS when it has the hidden flag (`UF_HIDDEN`, what hides `~/Library`). The
  folder the user added is never skipped as hidden: they chose it.
- **Tool-generated folders are recognised by evidence, not by common words.** A
  folder is skipped when it holds a `CACHEDIR.TAG` whose first line is the
  Cache Directory Tagging Specification's signature; or it is named
  `node_modules` or `__pycache__`; or it is `target` beside a `Cargo.toml` or
  `pom.xml`, or `dist` or `build` beside a `package.json`. Nothing else is
  skipped by name: a user's own `Clients/target/plan.docx` is prepared. `.git`,
  `.cache` and `.venv` are covered by the hidden rule.
- **What is skipped has nothing prepared.** When a scan skips a file or folder,
  or the folder's own policy leaves a file out, the catalog rows for it are
  erased, as a narrowed folder's files are (RFC-064 §3.2), and never marked
  missing (RFC-064: out of the folder is not missing).
- **§7's question also covers `AppData` (Windows) and `Library` (macOS)** when
  they are the folder directly under the home directory, not any folder of that
  name. The home directory is resolved once, by the runtime context.

Where: `crates/data/fs/src/policy.rs` (`is_generated_folder`,
`TOOL_OUTPUT_FOLDERS`, `platform_hidden`), `scanner.rs` (`erase_left_out`),
`sensitive.rs` (`in_home_application_data`).

## 11. Path Canonicalization Strategy

Implementation should use platform-aware canonicalization.

Potential issues:

- case-insensitive filesystems;
- symlinks;
- deleted path during registration;
- permission-denied parent directories;
- Windows drive letters;
- UNC paths;
- macOS normalization.

The canonical path should be stored separately from the display path.

---

## 12. Security Considerations

## 12.1. Local API Risk

If the UI uses a local HTTP API, a malicious webpage may attempt to call it.

Mitigations:

- bind to loopback only;
- require app-local CSRF token or equivalent;
- reject unknown origins;
- do not allow arbitrary path read endpoints.

## 12.2. Symlink Risk

Symlinks can escape an approved folder.

Mitigation:

- default `ignore`;
- if following symlinks, verify resolved path remains inside source root;
- record symlink traversal in scan metadata if useful.

## 12.3. Hidden and Secret Files

Local indexes can leak metadata or derived content to anyone with access to the user profile.

Mitigation:

- default hidden file exclusion;
- warning for sensitive directories;
- clear storage UI;
- privacy settings for query/snippet retention.

---

## 13. Acceptance Criteria

1. User can add persistent folder source.
2. User can add temporary file source.
3. Backend canonicalizes paths.
4. Backend rejects file reads outside active sources.
5. Hidden files are excluded by default.
6. Symlinks are ignored by default.
7. Sensitive directory warning is shown.
8. Source removal does not delete source files.
9. Source status supports active, paused, missing, permission denied, removed.
10. Tests cover path traversal and symlink escape attempts.

---

## 14. Testing Requirements

Required tests:

1. Add valid directory source.
2. Add valid file source.
3. Reject nonexistent path or mark missing.
4. Reject path traversal read.
5. Reject frontend request for non-source path.
6. Hidden file excluded by default.
7. Symlink outside source ignored.
8. Source can be paused/resumed.
9. Temporary source cleanup removes its indexes, not source file.
10. Sensitive path warning triggered.

---

## 15. Unresolved Questions

- Should temporary sources persist across app restart by default?
- Should source policies be inherited by nested sources?
- Should source priorities influence search ranking?
- Should users be able to import/export source policies?
- How much path information should be shown in logs?

---

## 16. Decision

Adopt allowlist-based source registration with backend-enforced path validation.

The frontend may request source operations, but the backend is the authority for all file access.
