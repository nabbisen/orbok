# RFC-011: Storage Dashboard and Cleanup UX

**Project:** orbok  
**RFC:** 011  
**Title:** Storage Dashboard and Cleanup UX  
**Status:** Implemented (v0.4.0)
**Target Milestone:** M10  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines the Storage Dashboard and cleanup UX for `orbok`.

The central decision is:

> Storage management must be lifecycle-aware. The UI must distinguish persistent catalog data, rebuildable index data, ephemeral cache data, local model files, and logs before any cleanup action is allowed.

This RFC turns the data lifecycle model from RFC-001 into concrete product behavior.

---

## 2. Motivation

`orbok` is storage-conscious by design. It avoids duplicating source files, but it still stores derived data:

- file catalog records;
- chunk metadata;
- keyword indexes;
- embeddings;
- vector index files;
- snippets;
- search caches;
- model files;
- `localcache` payloads.

If the UI simply says “clear cache” or “delete data,” users may misunderstand what is safe to remove, what requires rebuilding, and what affects search quality.

Storage transparency is central to user trust.

---

## 3. Goals

- Show how much local storage `orbok` uses.
- Break storage down by meaningful lifecycle categories.
- Provide safe cleanup actions.
- Provide rebuildable-index cleanup actions with clear warnings.
- Prevent accidental deletion of persistent source configuration.
- Integrate `localcache` storage accounting.
- Explain that source files are never deleted by cleanup.
- Provide confirmation levels appropriate to impact.

---

## 4. Non-Goals

- This RFC does not define backup/export features.
- This RFC does not implement vector compression.
- This RFC does not define OS-level disk cleanup.
- This RFC does not delete source files.
- This RFC does not provide secure deletion guarantees.

---

## 5. Storage Categories

The Storage Dashboard must display at least:

| Category | Lifecycle Class | Examples |
|---|---|---|
| Persistent catalog | Persistent catalog | sources, file records, settings, model registry |
| Exact search index | Rebuildable index | keyword/FTS/Tantivy index |
| Semantic search index | Rebuildable index | embeddings, vector segments |
| Temporary extraction cache | Rebuildable/ephemeral | localcache extracted segment payloads |
| Snippet cache | Ephemeral cache | preview snippets |
| Search cache | Ephemeral cache | query result cache |
| Model files | Local dependency | embedding/reranker models |
| Logs and diagnostics | Operational data | redacted app logs |

---

## 6. Storage Dashboard Layout

Recommended desktop layout:

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Storage                                                                      │
│ See what orbok stores and clean up safely.                                   │
├──────────────────────────────────────────────────────────────────────────────┤
│ Total orbok storage: 1.42 GB                                                  │
│                                                                              │
│ ┌───────────────────────────┐ ┌────────────────────────────────────────────┐ │
│ │ Storage Breakdown          │ │ Cleanup Actions                            │ │
│ │                           │ │                                            │ │
│ │ Persistent catalog  24 MB │ │ Safe cleanup                               │ │
│ │ Exact index        180 MB │ │ [Clear expired search cache]                │ │
│ │ Semantic index     920 MB │ │ [Clear temporary snippets]                  │ │
│ │ Extraction cache    32 MB │ │ [Remove replaced stale indexes]             │ │
│ │ Search cache        12 MB │ │                                            │ │
│ │ Models             260 MB │ │ Space recovery                             │ │
│ │ Logs                2 MB  │ │ [Delete semantic index and rebuild later]   │ │
│ └───────────────────────────┘ │ [Delete exact index and rebuild later]      │ │
│                               │                                            │ │
│                               │ Dangerous                                  │ │
│                               │ [Reset orbok catalog...]                   │ │
│                               └────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 7. Cleanup Action Classes

## 7.1. Safe Cleanup

Safe cleanup removes only data that is not required for correctness.

Examples:

- expired search cache;
- expired snippet cache;
- stale replaced index fragments;
- temporary extraction buffers;
- old rerank cache.

No strong confirmation required, but the action should still be visible.

## 7.2. Space Recovery Cleanup

Space recovery cleanup deletes rebuildable indexes.

Examples:

- delete semantic search index;
- delete exact search index;
- delete temporary source indexes;
- delete localcache chunk/embedding bundles.

Requires confirmation.

The UI must say:

```text
This will not delete your source files.
Search may be slower or incomplete until the index is rebuilt.
```

## 7.3. Destructive Reset

Destructive reset deletes persistent catalog data.

Examples:

- reset orbok catalog;
- remove all registered sources;
- remove source policies;
- remove file catalog.

Requires strong confirmation.

---

## 8. CleanupPlan API

All cleanup actions must first create a cleanup plan.

Conceptual model:

```rust
pub struct CleanupPlan {
    pub action: CleanupAction,
    pub affected_categories: Vec<StorageCategory>,
    pub affected_lifecycle_classes: Vec<DataClass>,
    pub estimated_recovered_bytes: u64,
    pub deletes_source_files: bool,
    pub requires_rebuild: bool,
    pub requires_confirmation: ConfirmationLevel,
    pub warnings: Vec<String>,
}
```

Rules:

- `deletes_source_files` must always be false for normal orbok cleanup.
- UI must display plan before high-risk actions.
- Backend must execute only plans it created.

---

## 9. Confirmation Levels

| Level | Use |
|---|---|
| none | expired cache cleanup |
| normal | remove temporary source index |
| strong | delete exact/semantic index |
| typed | reset catalog |

Typed confirmation example:

```text
Type RESET to confirm.
```

---

## 10. localcache Integration

`localcache` namespaces must map into storage categories.

Recommended mapping:

| localcache Namespace | Storage Category |
|---|---|
| `extract-segments:*` | Temporary extraction cache |
| `normalized-text:*` | Temporary extraction cache |
| `chunk-bundle:*` | Rebuildable index |
| `embedding-bundle:*` | Semantic search index |
| `preview-cache:*` | Snippet cache |

The Storage Manager must query `localcache` stats through the `orbok` cache service wrapper, not directly from UI code.

Cleanup must call:

- expired cleanup;
- stale-version cleanup;
- missing-file cleanup;
- namespace deletion;
- database shrink where appropriate.

---

## 11. Storage Accounting

`storage_accounting` table should be updated by:

- scheduled recalculation;
- after cleanup actions;
- after index build/rebuild;
- after model install/remove;
- after localcache cleanup.

Categories:

```text
persistent_catalog
keyword_index
vector_index
temporary_extraction
snippet_cache
search_cache
model_files
logs
```

---

## 12. UI Copy Requirements

Use clear wording:

```text
Source files will not be deleted.
This data can be rebuilt from your source files.
Semantic search may be unavailable until rebuilding completes.
This action removes registered source settings.
```

Avoid vague labels:

```text
Delete data
Clear database
Remove files
Clean everything
```

---

## 13. Empty and Error States

## 13.1. Storage Accounting Unknown

```text
Storage usage has not been calculated yet.
[Calculate Now]
```

## 13.2. Cache Database Missing

```text
The cache database is missing or was removed.
orbok can recreate it automatically.
[Recreate Cache]
```

## 13.3. Cache Database Corrupt

```text
The cache database appears to be damaged.
You can rebuild cache data from source files.
[Rebuild Cache]
```

---

## 14. Acceptance Criteria

- Storage Dashboard shows all required categories.
- Safe cleanup never deletes persistent catalog data.
- Cleanup plan is generated before cleanup execution.
- Source files are never deleted by cleanup.
- Deleting semantic index marks rebuild required.
- Deleting exact index marks rebuild required.
- Reset catalog requires strong typed confirmation.
- `localcache` stats appear in storage accounting.
- Text-bearing caches can be deleted.
- Model files are shown separately from indexes.

---

## 15. Testing Requirements

Required tests:

1. Safe cleanup preserves sources.
2. Safe cleanup removes expired snippet cache.
3. Delete semantic index preserves file catalog.
4. Delete exact index preserves source settings.
5. Reset catalog removes sources only after strong confirmation.
6. Cleanup plan reports no source-file deletion.
7. localcache namespace size is included.
8. Corrupt cache database can be rebuilt.
9. Privacy-strict mode clears text-bearing caches.
10. UI copy distinguishes source files from orbok indexes.

---

## 16. Unresolved Questions

- Should storage accounting be exact or approximate by default?
- Should localcache database shrink run automatically?
- Should model files be removable from Storage or only Models view?
- Should vector compression be exposed here or in advanced settings?
- Should backup/export include rebuildable indexes?

---

## 17. Decision

Implement a lifecycle-aware Storage Dashboard before release.

All cleanup must be mediated by a backend-generated `CleanupPlan`.
