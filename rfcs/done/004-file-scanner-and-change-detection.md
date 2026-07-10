# RFC-004: File Scanner and Change Detection

**Project:** orbok  
**RFC:** 004  
**Title:** File Scanner and Change Detection  
**Status:** Implemented (v0.1.0)
**Target Milestone:** M3  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines how `orbok` scans registered sources, catalogs files, and detects file changes.

The scanner is responsible for turning approved sources into file catalog records with accurate lifecycle states such as discovered, indexed, stale, missing, deleted, unsupported, permission denied, and failed.

`localcache` may perform freshness checks for cached file-derived payloads, but it does not replace the scanner.

---

## 2. Motivation

A local search app must handle a changing filesystem:

- files are created;
- files are modified;
- files are renamed;
- files are deleted;
- external drives disappear;
- permissions change;
- cloud-synced folders are temporarily inconsistent.

Without reliable change detection, search results become stale, snippets point to wrong offsets, and embeddings become incompatible with source content.

---

## 3. Goals

- Recursively scan registered sources.
- Respect source policies.
- Catalog supported and unsupported files.
- Detect new, changed, missing, deleted, and permission-denied files.
- Mark stale indexes before reindexing.
- Avoid full reindexing when files are unchanged.
- Isolate failures per file.
- Queue downstream index jobs.

---

## 4. Non-Goals

- This RFC does not implement document extraction.
- This RFC does not implement chunking.
- This RFC does not implement file watching as a mandatory feature.
- This RFC does not implement content search.
- This RFC does not implement cloud sync integration.

---

## 5. Scanner Inputs

Scanner input:

```text
source_id
source canonical path
source type
include patterns
exclude patterns
hidden file policy
symlink policy
max file size
supported extension policy
previous file catalog state
```

---

## 6. Scanner Outputs

Scanner output:

```text
new file records
updated file records
missing/deleted file markers
permission errors
unsupported file records
index jobs for changed files
scan summary
```

---

## 7. File Status Model

Allowed file statuses:

| Status | Meaning |
|---|---|
| discovered | File found but not yet indexed |
| indexed | File indexed and current |
| stale | File changed after indexing |
| missing | Previously known file is not currently available |
| deleted | File removed from catalog after cleanup or confirmed deletion |
| permission_denied | File exists but cannot be read |
| unsupported | File type not supported or excluded |
| failed | Scanner or indexing failure |

---

## 8. Relationship with localcache

The scanner remains authoritative for source traversal and file catalog state.

`localcache` can be used later by indexing workers to decide whether a cached payload for a file is still fresh. This is an optimization after source policy validation and scan scheduling.

Do not use `localcache::scan_dir_filtered` as the primary scanner in v1 because `orbok` must enforce its own source policies, hidden-file behavior, symlink policy, unsupported-file state, and UI-visible scan summaries.

## 9. Change Detection Strategy

## 8.1. Fast Metadata Check

Initial comparison:

- canonical path;
- file size;
- modified timestamp;
- platform file key where available.

If unchanged, skip expensive hash.

## 8.2. Content Hash

When a file is new, suspicious, or selected for indexing, compute content hash.

Recommended hash:

```text
sha256
```

Hashing may be skipped for very large files until extraction/indexing phase, but a file should not be considered fully indexed without a stable content identity.

## 8.3. Platform File Key

Where available, store platform identity:

- inode/device on Unix-like systems;
- file index/volume serial on Windows.

This helps distinguish rename/move from delete+create, but the app must not depend on it exclusively.

---

## 10. Scan Algorithm

```text
for each active source:
    validate source path
    enumerate files according to policy
    for each discovered path:
        canonicalize path
        check source membership
        check hidden policy
        check symlink policy
        check include/exclude patterns
        check max file size
        classify file type
        compare with file catalog
        insert or update file record
        queue index job if new or changed
    mark previously known files not seen as missing
```

---

## 11. Missing vs Deleted

The scanner should initially mark absent files as `missing`, not immediately `deleted`.

Reasons:

- external drive may be disconnected;
- cloud sync may be incomplete;
- permission may temporarily fail;
- path may reappear.

A file becomes `deleted` only after:

- user confirms cleanup;
- source is removed with associated data;
- retention policy expires for missing files;
- explicit advanced cleanup runs.

---

## 12. Stale Detection

A file becomes stale when:

- content hash changes;
- file size changes and hash cannot confirm equivalence;
- modified timestamp changes and policy requires recheck;
- extractor version changes;
- normalization version changes;
- chunker version changes;
- keyword tokenizer version changes;
- embedding model version changes.

This RFC covers only file content and metadata triggers. Extractor/model triggers are used by later stages.

---

## 13. Index Job Creation

When a file is new or stale, scanner creates jobs:

```text
extract
chunk
keyword_index
embedding
```

The exact job decomposition may be refined, but the scanner must at least mark the file for indexing.

Recommended initial behavior:

```text
scan job -> file discovered/stale -> extract job queued
```

Downstream stages may enqueue chunk/index/embedding jobs.

---

## 14. Source Scan Summary

Each scan should produce summary counts:

```text
seen_files
new_files
unchanged_files
stale_files
missing_files
unsupported_files
permission_denied_files
failed_files
queued_index_jobs
duration_ms
```

This summary should be visible in the Indexing view and event log.

---

## 15. Performance Considerations

## 14.1. Large Directory Trees

Scanner must avoid blocking UI.

Use:

- background job;
- progress updates;
- cancellation;
- configurable exclude patterns;
- max file size.

## 14.2. Hashing Cost

Content hashing can be expensive.

Recommended:

- metadata precheck first;
- hash only when needed;
- stream file content;
- support cancellation.

## 14.3. File Watcher

File watching may be added later. It is not required for initial scanner MVP.

Initial design may use manual or scheduled rescan.

---

## 16. Error Handling

Scanner errors are per file or per source.

Error categories:

```text
source_missing
permission_denied
path_canonicalization_failed
symlink_policy_blocked
file_too_large
unsupported_type
read_error
hash_error
internal_error
```

Errors must not stop scanning unrelated files.

---

## 17. UI Requirements

The Indexing view must show:

- current source being scanned;
- progress counts;
- failed files;
- permission-denied files;
- stale files;
- retry action;
- rescan action.

The Sources view must show:

- last scanned timestamp;
- source status;
- indexed file count;
- failed file count;
- stale file count.

---

## 18. Acceptance Criteria

- Scanner discovers files under active sources.
- Scanner respects include/exclude patterns.
- Scanner excludes hidden files by default.
- Scanner ignores symlinks according to policy.
- Scanner detects new files.
- Scanner detects modified files as stale.
- Scanner marks missing files without deleting records.
- Scanner records permission-denied files.
- Scanner queues indexing work for new/stale files.
- Scanner can be canceled without corrupting catalog state.

---

## 19. Testing Requirements

Required tests:

1. Empty source scan.
2. Source with new files.
3. Modified file becomes stale.
4. Deleted file becomes missing.
5. Missing file restored.
6. Permission denied file handled.
7. Excluded directory skipped.
8. Hidden file excluded.
9. Symlink outside source ignored.
10. Max file size enforced.
11. Scanner cancellation leaves valid database state.
12. Repeated scan is idempotent for unchanged files.

---

## 20. Implementation Notes

Recommended Rust modules:

```text
orbok-fs::scanner
orbok-fs::policy
orbok-fs::path_guard
orbok-db::repositories::files
orbok-core::jobs
```

Recommended scanner interface:

```rust
pub struct ScanRequest {
    pub source_id: SourceId,
    pub force_hash: bool,
    pub enqueue_index_jobs: bool,
}

pub struct ScanSummary {
    pub seen_files: u64,
    pub new_files: u64,
    pub stale_files: u64,
    pub missing_files: u64,
    pub failed_files: u64,
}
```

---

## 21. Unresolved Questions

- Should file watching be introduced before or after M6?
- Should very large files be partially indexed?
- Should unsupported files be cataloged or ignored entirely?
- How long should missing files remain before suggested cleanup?
- Should source-code repositories have special default excludes?

---

## 22. Decision

Implement manual/incremental scanner first.

Defer real-time file watching until the catalog and indexing pipeline are stable.


---

## 23. Amendment: localcache Reference

See `appendices/APPENDIX-A-localcache-integration.md`.

Normative summary:

- scanner owns source traversal and file catalog state;
- `localcache` freshness checks are worker-level optimizations;
- localcache entries must only be created for files approved by the source boundary.
