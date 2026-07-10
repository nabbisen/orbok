# RFC-001: Local Data Classification and Lifecycle

**Project:** orbok  
**RFC:** 001  
**Title:** Local Data Classification and Lifecycle  
**Status:** Implemented (v0.1.0)
**Target Milestone:** M1  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines the local data classification model for `orbok`.

The core decision is to classify all application-managed data into three lifecycle classes:

1. **Persistent catalog data**
2. **Rebuildable index data**
3. **Ephemeral cache data**

This classification is foundational. It determines database design, cleanup behavior, UI wording, storage accounting, backup expectations, and user trust.

`orbok` may use the `localcache` crate for file-derived caches, but such data must still be classified under this lifecycle model.

---

## 2. Motivation

`orbok` is designed to avoid duplicating source files while still providing high-quality local search. This requires storing derived data such as file metadata, chunk locations, keyword indexes, and embeddings.

Without explicit lifecycle classification, several risks appear:

- cleanup may delete important configuration;
- storage UI may mislead users;
- future RFCs may treat all SQLite data as “cache”;
- rebuild behavior may be unclear;
- privacy expectations may be violated;
- stale search results may be difficult to explain.

The application needs a strict vocabulary for what it stores and why.

---

## 3. Goals

- Define lifecycle classes for all local data.
- Make cleanup semantics safe and predictable.
- Ensure source files are never deleted by index cleanup.
- Enable storage reporting by data category.
- Establish language for later database and UI RFCs.
- Clarify what can be rebuilt from source files.
- Clarify what must not be removed by ordinary cleanup.

---

## 4. Non-Goals

- This RFC does not define the full SQLite schema.
- This RFC does not choose keyword search engine implementation.
- This RFC does not choose vector storage format.
- This RFC does not define final GUI layout.
- This RFC does not define backup/export features.

---

## 5. Definitions

## 5.1. Source File

A user-owned file on the local filesystem.

`orbok` may read source files only when they are under user-approved sources and policy allows access.

`orbok` must not delete source files.

## 5.2. Source

A user-approved file or directory that `orbok` may scan.

A source may be:

- persistent;
- temporary.

## 5.3. Persistent Catalog Data

Data required to remember the user's configuration and the current known state of indexed sources.

Examples:

- registered sources;
- source policies;
- file catalog records;
- index status records;
- model registry;
- application settings;
- schema migration records.

Persistent catalog data is not safe to delete through ordinary cleanup.

## 5.4. Rebuildable Index Data

Data generated from source files and local models that can be recreated.

Examples:

- keyword index;
- chunk-derived search records;
- embeddings;
- vector index segments;
- extracted-text-derived token indexes;
- stale index replacements.

Deleting rebuildable data may reduce search functionality until reindexing completes.

## 5.5. Ephemeral Cache Data

Data kept only for speed, convenience, or temporary UI display.

Examples:

- search result cache;
- temporary snippets;
- rerank scores;
- temporary extraction buffers;
- UI convenience cache.

Ephemeral cache data may be deleted automatically.

---

## 6. Data Classification Table

| Data | Class | Safe Ordinary Cleanup? | Rebuild Required? |
|---|---|---:|---:|
| Registered source path | Persistent catalog | No | User must reconfigure |
| Source include/exclude policy | Persistent catalog | No | User must reconfigure |
| File metadata | Persistent catalog | No | Rescan required |
| File content hash | Persistent catalog | No | Rescan/hash required |
| Extraction record | Persistent catalog / rebuildable boundary | Usually no | Re-extract required |
| Chunk metadata | Rebuildable index | With confirmation | Rechunk required |
| Chunk locations | Rebuildable index | With confirmation | Re-extract/rechunk required |
| Keyword index | Rebuildable index | With confirmation | Rebuild keyword index |
| Embeddings | Rebuildable index | With confirmation | Re-embed |
| Vector index segments | Rebuildable index | With confirmation | Rebuild vector index |
| Search result cache | Ephemeral cache | Yes | No |
| Snippet cache | Ephemeral cache | Yes | No |
| Rerank cache | Ephemeral cache | Yes | No |
| Model files | Local dependency | With strong confirmation | Reinstall/relocate |
| Logs | Operational data | Yes, with policy | No |

---

## 7. Lifecycle Rules

## 7.1. Persistent Catalog Rules

Persistent catalog data:

- must not be deleted by one-click cleanup;
- must be included in database migration tests;
- should survive cache cleanup;
- should have explicit reset behavior;
- should be small relative to indexes;
- may contain sensitive metadata such as paths.

## 7.2. Rebuildable Index Rules

Rebuildable index data:

- may be deleted to recover space;
- must be rebuildable from source files and model configuration;
- must be versioned by extractor/model/tokenizer where applicable;
- must be invalidated when inputs change;
- should be deleted only with user-visible explanation.

## 7.3. Ephemeral Cache Rules

Ephemeral cache data:

- may be deleted automatically by TTL or LRU;
- must not be required for correctness;
- should be excluded from backup by default;
- may be disabled or minimized by privacy settings;
- must be invalidated when source files change.

---

## 8. localcache Classification

`localcache`-managed data belongs only to one of the following classes:

- rebuildable index data;
- ephemeral cache data.

It must not be used as persistent catalog data.

Examples:

| localcache Payload | orbok Lifecycle Class |
|---|---|
| Extracted segment bundle | Rebuildable index or ephemeral cache, depending on retention |
| Chunk bundle | Rebuildable index |
| Per-file embedding bundle | Rebuildable index |
| Preview helper payload | Ephemeral cache |
| Temporary document analysis | Ephemeral cache |

The authoritative source list, file catalog, index job state, and user settings must remain in the orbok catalog.

## 9. Cleanup Policy

## 8.1. Safe Cleanup

Safe cleanup may remove:

- expired search cache;
- expired snippet cache;
- rerank cache;
- temporary extraction buffers;
- obsolete replaced indexes.

Safe cleanup must not remove:

- registered sources;
- source policies;
- active file catalog;
- model registry;
- active indexes unless explicitly selected.

## 8.2. Space-Recovery Cleanup

Space-recovery cleanup may remove:

- keyword index;
- vector index;
- embeddings;
- temporary source indexes.

The UI must explain that search may become unavailable, slower, or less accurate until rebuilding completes.

## 8.3. Reset Catalog

Reset catalog is destructive.

It may remove:

- sources;
- settings;
- file catalog;
- chunks;
- indexes;
- caches;
- search history.

It must not delete source files.

It should require strong confirmation.

---

## 10. Storage Accounting Categories

The Storage view should report at least:

```text
persistent_catalog
keyword_index
vector_index
snippet_cache
search_cache
temporary_extraction
model_files
logs
```

Each category should have:

- size in bytes;
- item count where meaningful;
- last recalculated timestamp;
- cleanup eligibility;
- rebuild implications.

---

## 11. UI Language

User-facing labels should avoid ambiguous phrases.

Use:

- “Clear temporary cache”
- “Delete semantic search index”
- “Rebuild exact search index”
- “Reset orbok catalog”
- “Source files will not be deleted”

Avoid:

- “Delete data” without qualification
- “Clear database”
- “Remove files” when source files are not affected
- “Cache DB” for the entire SQLite catalog

---

## 12. Acceptance Criteria

- Data classes are represented in code as explicit enum or equivalent.
- Cleanup functions require a target lifecycle class.
- Ordinary cleanup cannot delete persistent source settings.
- Storage usage is reportable by lifecycle category.
- Tests prove persistent data survives safe cleanup.
- User-facing copy distinguishes source files from orbok indexes.
- Rebuildable data deletion marks required reindexing state.

---

## 13. Testing Requirements

Required tests:

1. Safe cleanup preserves sources.
2. Safe cleanup removes expired snippet cache.
3. Deleting vector index does not remove file catalog.
4. Reset catalog does not touch source files.
5. Stale indexes are removable only after replacement or explicit confirmation.
6. Storage accounting recomputes after cleanup.
7. Privacy mode can disable query text retention.

---

## 14. Implementation Notes

Recommended Rust model:

```rust
pub enum DataClass {
    PersistentCatalog,
    RebuildableIndex,
    EphemeralCache,
    LocalDependency,
    OperationalLog,
}
```

Recommended cleanup shape:

```rust
pub struct CleanupPlan {
    pub action: CleanupAction,
    pub affected_classes: Vec<DataClass>,
    pub estimated_recovered_bytes: u64,
    pub requires_rebuild: bool,
    pub requires_confirmation: bool,
}
```

No cleanup operation should run without first producing a `CleanupPlan`.

---

## 15. Unresolved Questions

- Should model files be classified separately from persistent catalog data?
- Should search history be disabled by default?
- Should temporary sources survive app restart by default?
- Should snippet cache be stored in SQLite or external files?
- Should backups include rebuildable indexes?

---

## 16. Decision

Adopt the three-class lifecycle model:

1. persistent catalog;
2. rebuildable index;
3. ephemeral cache.

This classification is mandatory for database design, cleanup design, and UI wording.


---

## 17. Amendment: localcache Reference

See `appendices/APPENDIX-A-localcache-integration.md`.

Normative summary:

- `localcache` may manage rebuildable and ephemeral file-derived payloads.
- `localcache` must be accessed through an `orbok` cache service wrapper.
- `localcache` cleanup must be driven by an `orbok` cleanup plan.
- `localcache` data must be reported in the Storage view by lifecycle category.
