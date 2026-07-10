# RFC-018: Crash Recovery, Diagnostics, and Repair Tools

**Project:** orbok  
**RFC:** 018  
**Title:** Crash Recovery, Diagnostics, and Repair Tools  
**Status:** Implemented (v0.5.0)
**Target Milestone:** M13  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines crash recovery, diagnostics, and repair tooling for `orbok`.

The central decision is:

> `orbok` must assume indexing can be interrupted. It must recover without corrupting the catalog, losing source settings, or presenting stale data as fresh.

---

## 2. Motivation

`orbok` performs long-running local operations:

- scanning large folders;
- extracting PDFs/DOCX files;
- chunking large documents;
- building keyword indexes;
- generating embeddings;
- compacting vector indexes;
- cleaning caches.

These operations can be interrupted by crashes, shutdowns, permission changes, disk-full errors, or user cancellation.

Without repair tooling, a local-first app can become untrustworthy.

---

## 3. Goals

- Recover from interrupted indexing.
- Detect incomplete jobs.
- Preserve previous active indexes when replacement fails.
- Repair or recreate cache database.
- Validate catalog integrity.
- Provide safe diagnostics export.
- Avoid leaking document contents in diagnostics.
- Provide CLI or internal repair commands.

---

## 4. Non-Goals

- This RFC does not guarantee recovery from arbitrary disk corruption.
- This RFC does not implement secure deletion.
- This RFC does not replace user backups.
- This RFC does not define cloud sync.
- This RFC does not implement database replication.

---

## 5. Crash-Safe Indexing Principle

Use replace-on-success.

For each file reindex:

1. keep old active chunks/indexes;
2. create new extraction record;
3. create new chunks and derived indexes;
4. commit new active state;
5. mark old state stale/deleted;
6. cleanup later.

If any stage fails, old active state remains usable.

---

## 6. Startup Recovery

On startup:

```text
open catalog
verify schema
find running jobs from previous session
mark interrupted jobs as failed or queued_for_retry
verify active source records
verify cache database availability
verify storage accounting freshness
show recovery notice if needed
```

Interrupted job statuses:

- running -> failed_interrupted or queued;
- building index segment -> stale/failed;
- temporary transaction artifacts -> cleanup candidate.

---

## 7. Repair Commands

Recommended CLI/internal commands:

```text
orbok repair catalog-check
orbok repair cache-rebuild
orbok repair storage-recount
orbok repair mark-stale
orbok repair remove-orphaned-indexes
orbok repair rebuild-keyword-index
orbok repair rebuild-vector-index
orbok diagnostics export
```

These may be exposed through UI later.

---

## 8. Catalog Integrity Check

Checks:

- foreign key integrity;
- orphan chunks;
- orphan embeddings;
- files without source;
- active chunks for missing files;
- active embeddings for stale chunks;
- model references missing;
- failed migrations;
- storage accounting mismatch.

Output should be human-readable and machine-readable.

---

## 9. Cache Recovery

`localcache` cache database is rebuildable.

If missing:

```text
recreate cache database
mark cache as empty
continue
```

If corrupt:

```text
rename corrupt cache file
create new cache database
show warning
continue
```

Do not fail app startup because of cache corruption unless required by a future strict mode.

---

## 10. Disk Full Handling

Disk full can happen during:

- extraction cache write;
- keyword indexing;
- embedding storage;
- vector segment write;
- log write.

Behavior:

- fail current job;
- preserve previous active index;
- record error;
- show storage warning;
- suggest cleanup.

---

## 11. Diagnostics Export

Diagnostics export should help developers without leaking document contents.

Default export includes:

- app version;
- OS;
- schema version;
- settings summary with redaction;
- source count, not full paths by default;
- index counts;
- error categories;
- recent redacted events;
- storage accounting;
- model status;
- benchmark summary if available.

Default export excludes:

- document text;
- snippets;
- embeddings;
- raw search queries;
- full file paths unless user opts in.

---

## 12. Redaction Policy

Redaction options:

| Data | Default |
|---|---|
| full paths | redacted or shortened |
| source names | redacted |
| document snippets | excluded |
| query text | excluded if history disabled |
| model names | included |
| error categories | included |
| stack traces | included if no content |

User may choose “include full paths” explicitly.

---

## 13. Event Log Policy

Event log should store:

- event type;
- severity;
- redacted details;
- timestamp.

Avoid:

- document body;
- extracted text;
- vector values;
- unredacted secrets.

---

## 14. UI Recovery Notices

Examples:

```text
orbok recovered from an interrupted indexing job.
Some files were queued for retry.
[View Indexing]
```

```text
The cache database was damaged and has been recreated.
Search indexes can be rebuilt from source files.
[Open Storage]
```

```text
Storage accounting may be out of date.
[Recalculate]
```

---

## 15. Backup Before Migration

Before risky catalog migrations:

- create catalog backup;
- record backup path;
- allow rollback manually if migration fails;
- do not backup large rebuildable indexes by default unless needed.

---

## 16. Acceptance Criteria

- Interrupted jobs are detected on startup.
- Old active index survives failed reindex.
- Cache DB missing is recreated.
- Cache DB corrupt case does not destroy catalog.
- Storage accounting can be recalculated.
- Diagnostics export redacts content by default.
- Catalog integrity check exists.
- Repair commands are documented.
- Disk-full job failure is recoverable.
- Migration backup policy exists.

---

## 17. Testing Requirements

Required tests:

1. Crash during extraction leaves old index active.
2. Crash during chunk replacement leaves catalog valid.
3. Crash during embedding write does not corrupt chunks.
4. Startup marks interrupted jobs.
5. Missing localcache DB recreated.
6. Corrupt cache DB renamed/rebuilt.
7. Catalog integrity detects orphan embedding.
8. Diagnostics export excludes snippets.
9. Storage recount updates accounting.
10. Disk-full simulation fails job safely.

---

## 18. Unresolved Questions

- Should repair commands be CLI-only or exposed in GUI?
- Should catalog backups be compressed?
- How many migration backups should be retained?
- Should vector segment repair be separate?
- Should diagnostics export include hashed paths?

---

## 19. Decision

Implement recovery and diagnostics as first-class release-readiness features.

Do not rely on “delete the database and start over” as the only repair path.
