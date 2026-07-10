# RFC-003: Source Registration and File Access Boundary

**Project:** orbok  
**RFC:** 003  
**Title:** Source Registration and File Access Boundary  
**Status:** Implemented (v0.1.0)
**Target Milestone:** M2  
**Date:** 2026-06-06  

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

Recommended default excludes:

```text
.git
node_modules
target
dist
build
.cache
.venv
__pycache__
```

These should be configurable.

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

- User can add persistent folder source.
- User can add temporary file source.
- Backend canonicalizes paths.
- Backend rejects file reads outside active sources.
- Hidden files are excluded by default.
- Symlinks are ignored by default.
- Sensitive directory warning is shown.
- Source removal does not delete source files.
- Source status supports active, paused, missing, permission denied, removed.
- Tests cover path traversal and symlink escape attempts.

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
