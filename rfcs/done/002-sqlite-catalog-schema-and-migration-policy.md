# RFC-002: SQLite Catalog Schema and Migration Policy

**Project:** orbok  
**RFC:** 002  
**Title:** SQLite Catalog Schema and Migration Policy  
**Status:** Implemented (v0.1.0)
**Target Milestone:** M1  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines the initial SQLite catalog design for `orbok`.

SQLite is used as the local search catalog and state database. It is not merely a cache. It stores persistent catalog data, rebuildable index metadata, ephemeral cache records, migration state, and storage accounting.

---

## 2. Motivation

`orbok` requires a durable local database for:

- registered sources;
- source policies;
- file catalog;
- extraction records;
- chunk metadata;
- index jobs;
- model registry;
- app settings;
- cache records;
- storage accounting.

Using SQLite via `rusqlite` provides a lightweight, single-file, no-daemon storage layer appropriate for a local-first desktop application.

`orbok` may additionally use `localcache` as a separate SQLite-backed cache engine for file-derived rebuildable and ephemeral payloads. The `orbok` catalog and the `localcache` database should remain separate.

However, the database must be designed with clear lifecycle semantics so that cleanup, rebuilds, and migrations are safe.

---

## 3. Goals

- Define baseline SQLite schema.
- Establish migration policy.
- Enable foreign key integrity.
- Separate persistent, rebuildable, and ephemeral records.
- Support incremental indexing.
- Support stale/missing/deleted state.
- Support model and extractor versioning.
- Enable storage accounting.

---

## 4. Non-Goals

- This RFC does not choose the final keyword search engine.
- This RFC does not define final vector storage format.
- This RFC does not implement ANN indexing.
- This RFC does not define GUI screens.
- This RFC does not define all future document format tables.

---

## 5. Database Configuration

SQLite should be opened with:

```sql
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA temp_store = MEMORY;
```

Recommended behavior:

- Use one serialized writer path.
- Allow multiple read operations.
- Use transactions for indexing replacement.
- Run migrations before application services start.
- Avoid storing full document text by default.

---

## 6. Migration Policy

## 6.1. Migration Table

```sql
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL
);
```

## 6.2. Migration Rules

- Migrations are append-only.
- Each migration has a numeric version and stable name.
- Migrations must be idempotent at the runner level.
- Failed migrations must abort startup.
- Downgrades are not required for early development.
- Test databases must run all migrations from empty state.

## 6.3. Schema Versioning

Application startup must verify:

- current schema version;
- supported minimum schema version;
- pending migrations;
- migration failure state.

---

## 7. Core Tables

## 7.1. App Settings

```sql
CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

Settings are persistent catalog data.

---

## 7.2. Sources

```sql
CREATE TABLE sources (
    source_id TEXT PRIMARY KEY,
    source_type TEXT NOT NULL CHECK (source_type IN ('directory', 'file')),
    persistence_mode TEXT NOT NULL CHECK (persistence_mode IN ('persistent', 'temporary')),
    display_name TEXT,
    original_path TEXT NOT NULL,
    canonical_path TEXT NOT NULL,
    status TEXT NOT NULL CHECK (
        status IN ('active', 'paused', 'missing', 'permission_denied', 'removed')
    ),
    index_mode TEXT NOT NULL CHECK (
        index_mode IN ('balanced', 'high_accuracy', 'space_saving')
    ),
    include_patterns_json TEXT,
    exclude_patterns_json TEXT,
    hidden_file_policy TEXT NOT NULL CHECK (
        hidden_file_policy IN ('exclude', 'include', 'warn')
    ),
    symlink_policy TEXT NOT NULL CHECK (
        symlink_policy IN ('ignore', 'follow_within_source', 'follow_all_with_warning')
    ),
    max_file_size_bytes INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_scanned_at TEXT
);

CREATE INDEX idx_sources_status ON sources(status);
CREATE INDEX idx_sources_persistence ON sources(persistence_mode);
```

---

## 7.3. Files

```sql
CREATE TABLE files (
    file_id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES sources(source_id) ON DELETE CASCADE,
    original_path TEXT NOT NULL,
    canonical_path TEXT NOT NULL,
    display_path TEXT NOT NULL,
    extension TEXT,
    mime_type TEXT,
    file_size_bytes INTEGER NOT NULL,
    modified_at TEXT,
    platform_file_key TEXT,
    content_hash TEXT,
    hash_algorithm TEXT,
    file_status TEXT NOT NULL CHECK (
        file_status IN (
            'discovered',
            'indexed',
            'stale',
            'missing',
            'deleted',
            'permission_denied',
            'unsupported',
            'failed'
        )
    ),
    last_seen_at TEXT NOT NULL,
    last_scanned_at TEXT,
    last_indexed_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(source_id, canonical_path)
);

CREATE INDEX idx_files_source_id ON files(source_id);
CREATE INDEX idx_files_status ON files(file_status);
CREATE INDEX idx_files_hash ON files(content_hash);
CREATE INDEX idx_files_modified_at ON files(modified_at);
```

---

## 7.4. Extraction Records

```sql
CREATE TABLE extraction_records (
    extraction_id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL REFERENCES files(file_id) ON DELETE CASCADE,
    extractor_name TEXT NOT NULL,
    extractor_version TEXT NOT NULL,
    normalization_version TEXT NOT NULL,
    source_content_hash TEXT,
    status TEXT NOT NULL CHECK (
        status IN ('pending', 'running', 'succeeded', 'failed', 'obsolete')
    ),
    extracted_char_count INTEGER,
    extracted_byte_count INTEGER,
    error_category TEXT,
    error_message TEXT,
    started_at TEXT,
    completed_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_extraction_file_id ON extraction_records(file_id);
CREATE INDEX idx_extraction_status ON extraction_records(status);
```

---

## 7.5. Chunks

```sql
CREATE TABLE chunks (
    chunk_id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL REFERENCES files(file_id) ON DELETE CASCADE,
    extraction_id TEXT NOT NULL REFERENCES extraction_records(extraction_id) ON DELETE CASCADE,
    parent_chunk_id TEXT REFERENCES chunks(chunk_id) ON DELETE CASCADE,
    chunk_kind TEXT NOT NULL CHECK (
        chunk_kind IN ('document', 'section', 'paragraph', 'page', 'code_block', 'table', 'fallback')
    ),
    chunk_ordinal INTEGER NOT NULL,
    heading_path TEXT,
    title TEXT,
    token_count INTEGER,
    char_count INTEGER,
    content_hash TEXT,
    chunk_status TEXT NOT NULL CHECK (
        chunk_status IN ('active', 'stale', 'deleted', 'failed')
    ),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(file_id, extraction_id, chunk_ordinal)
);

CREATE INDEX idx_chunks_file_id ON chunks(file_id);
CREATE INDEX idx_chunks_parent ON chunks(parent_chunk_id);
CREATE INDEX idx_chunks_status ON chunks(chunk_status);
CREATE INDEX idx_chunks_hash ON chunks(content_hash);
```

---

## 7.6. Chunk Locations

```sql
CREATE TABLE chunk_locations (
    chunk_id TEXT PRIMARY KEY REFERENCES chunks(chunk_id) ON DELETE CASCADE,
    byte_start INTEGER,
    byte_end INTEGER,
    char_start INTEGER,
    char_end INTEGER,
    page_start INTEGER,
    page_end INTEGER,
    line_start INTEGER,
    line_end INTEGER,
    location_quality TEXT NOT NULL CHECK (
        location_quality IN ('exact', 'approximate', 'page_only', 'unknown')
    ),
    locator_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

---

## 7.7. Models

```sql
CREATE TABLE models (
    model_id TEXT PRIMARY KEY,
    role TEXT NOT NULL CHECK (role IN ('embedding', 'reranker')),
    model_name TEXT NOT NULL,
    model_version TEXT NOT NULL,
    model_family TEXT,
    local_path TEXT,
    license_summary TEXT,
    size_bytes INTEGER,
    backend TEXT,
    dimension INTEGER,
    status TEXT NOT NULL CHECK (
        status IN ('available', 'missing', 'invalid', 'installing', 'disabled')
    ),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_validated_at TEXT
);

CREATE UNIQUE INDEX idx_models_role_name_version
ON models(role, model_name, model_version);
```

---

## 7.8. Embeddings Metadata

Actual vector storage may be SQLite BLOB or external file. This table stores metadata either way.

```sql
CREATE TABLE embeddings (
    embedding_id TEXT PRIMARY KEY,
    chunk_id TEXT NOT NULL REFERENCES chunks(chunk_id) ON DELETE CASCADE,
    model_id TEXT NOT NULL REFERENCES models(model_id),
    vector_format TEXT NOT NULL CHECK (
        vector_format IN ('fp32', 'fp16', 'int8', 'binary')
    ),
    dimension INTEGER NOT NULL,
    norm TEXT NOT NULL CHECK (norm IN ('l2', 'none', 'unknown')),
    storage_location TEXT NOT NULL CHECK (
        storage_location IN ('sqlite_blob', 'external_file')
    ),
    vector_blob BLOB,
    external_path TEXT,
    vector_hash TEXT,
    status TEXT NOT NULL CHECK (
        status IN ('active', 'stale', 'deleted', 'failed')
    ),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(chunk_id, model_id, vector_format)
);

CREATE INDEX idx_embeddings_chunk_id ON embeddings(chunk_id);
CREATE INDEX idx_embeddings_model_id ON embeddings(model_id);
CREATE INDEX idx_embeddings_status ON embeddings(status);
```

---

## 7.9. Index Jobs

```sql
CREATE TABLE index_jobs (
    job_id TEXT PRIMARY KEY,
    source_id TEXT REFERENCES sources(source_id) ON DELETE CASCADE,
    file_id TEXT REFERENCES files(file_id) ON DELETE CASCADE,
    job_type TEXT NOT NULL CHECK (
        job_type IN (
            'scan',
            'extract',
            'chunk',
            'keyword_index',
            'embedding',
            'delete_stale',
            'rebuild'
        )
    ),
    status TEXT NOT NULL CHECK (
        status IN (
            'queued',
            'running',
            'succeeded',
            'failed',
            'canceled',
            'blocked'
        )
    ),
    priority INTEGER NOT NULL DEFAULT 0,
    progress_current INTEGER NOT NULL DEFAULT 0,
    progress_total INTEGER,
    error_category TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT
);

CREATE INDEX idx_index_jobs_status ON index_jobs(status);
CREATE INDEX idx_index_jobs_file_id ON index_jobs(file_id);
CREATE INDEX idx_index_jobs_source_id ON index_jobs(source_id);
```

---

## 7.10. Search Cache

```sql
CREATE TABLE search_queries (
    query_id TEXT PRIMARY KEY,
    query_text TEXT,
    query_hash TEXT NOT NULL,
    mode TEXT NOT NULL,
    source_filter_json TEXT,
    created_at TEXT NOT NULL,
    expires_at TEXT
);

CREATE TABLE search_result_cache (
    cache_id TEXT PRIMARY KEY,
    query_id TEXT NOT NULL REFERENCES search_queries(query_id) ON DELETE CASCADE,
    chunk_id TEXT REFERENCES chunks(chunk_id) ON DELETE SET NULL,
    rank INTEGER NOT NULL,
    keyword_rank INTEGER,
    vector_rank INTEGER,
    rrf_score REAL,
    rerank_score REAL,
    source_status_at_query TEXT,
    created_at TEXT NOT NULL,
    last_accessed_at TEXT NOT NULL,
    expires_at TEXT
);

CREATE INDEX idx_search_cache_query ON search_result_cache(query_id);
CREATE INDEX idx_search_cache_last_accessed ON search_result_cache(last_accessed_at);
```

---

## 7.11. Snippet Cache

```sql
CREATE TABLE snippet_cache (
    snippet_id TEXT PRIMARY KEY,
    chunk_id TEXT REFERENCES chunks(chunk_id) ON DELETE CASCADE,
    file_content_hash TEXT,
    snippet_text TEXT NOT NULL,
    highlight_ranges_json TEXT,
    created_at TEXT NOT NULL,
    last_accessed_at TEXT NOT NULL,
    expires_at TEXT,
    size_bytes INTEGER NOT NULL
);

CREATE INDEX idx_snippet_cache_chunk ON snippet_cache(chunk_id);
CREATE INDEX idx_snippet_cache_last_accessed ON snippet_cache(last_accessed_at);
CREATE INDEX idx_snippet_cache_expires ON snippet_cache(expires_at);
```

---

## 7.12. Storage Accounting

```sql
CREATE TABLE storage_accounting (
    category TEXT PRIMARY KEY,
    size_bytes INTEGER NOT NULL,
    item_count INTEGER NOT NULL,
    updated_at TEXT NOT NULL
);
```

---

## 7.13. App Events

```sql
CREATE TABLE app_events (
    event_id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    severity TEXT NOT NULL CHECK (
        severity IN ('debug', 'info', 'warning', 'error')
    ),
    message TEXT NOT NULL,
    redacted_details_json TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_app_events_type ON app_events(event_type);
CREATE INDEX idx_app_events_created_at ON app_events(created_at);
```

---


---

## 7.14. Cache Engine Registry

The `orbok` catalog may track cache engines and namespaces used by external/cache-layer storage such as `localcache`.

This table is metadata about cache usage. It does not duplicate or manage `localcache`'s internal schema.

```sql
CREATE TABLE cache_engines (
    cache_engine_id TEXT PRIMARY KEY,
    engine_kind TEXT NOT NULL CHECK (
        engine_kind IN ('localcache')
    ),
    database_path TEXT NOT NULL,
    namespace TEXT NOT NULL,
    data_class TEXT NOT NULL CHECK (
        data_class IN ('rebuildable_index', 'ephemeral_cache')
    ),
    payload_type TEXT NOT NULL,
    payload_version INTEGER NOT NULL,
    ttl_seconds INTEGER,
    max_entries INTEGER,
    status TEXT NOT NULL CHECK (
        status IN ('active', 'disabled', 'missing', 'corrupt')
    ),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(engine_kind, database_path, namespace)
);

CREATE INDEX idx_cache_engines_status ON cache_engines(status);
CREATE INDEX idx_cache_engines_data_class ON cache_engines(data_class);
```

Recommended file split:

```text
orbok-catalog.sqlite3
orbok-cache.sqlite3
```

The `localcache` database must not share `orbok`'s migration table or `PRAGMA user_version`.


## 8. Repository Layer

The Rust implementation should not spread SQL directly across the app.

Recommended repositories:

```text
SettingsRepository
SourceRepository
FileRepository
CacheEngineRepository
ExtractionRepository
ChunkRepository
ModelRepository
EmbeddingRepository
IndexJobRepository
CacheRepository
StorageAccountingRepository
EventRepository
```

Each repository should expose application-level types rather than raw SQL rows.

---

## 9. Transaction Policy

Required transactions:

1. Add source.
2. Mark source removed.
3. Replace file index on successful reindex.
4. Mark old chunks stale after new chunks succeed.
5. Delete rebuildable indexes.
6. Cleanup expired cache.
7. Apply migration.

Indexing a file should use replace-on-success behavior.

Do not delete the old active chunks before new extraction/chunking/indexing succeeds.

---

## 10. localcache Integration Requirements

If `localcache` is used:

- use a separate database file;
- define one namespace per payload family;
- store namespace metadata in `cache_engines` or equivalent settings;
- route all calls through an orbok-owned cache service;
- never treat `localcache` as the source catalog;
- account for localcache storage in `storage_accounting`.

## 11. Acceptance Criteria

- Empty database can be initialized by migrations.
- Foreign keys are enabled and tested.
- Source deletion cascades correctly only where intended.
- Safe cleanup does not delete persistent settings.
- File status transitions are representable.
- Extraction version and model version compatibility are representable.
- Search history can omit raw query text.
- Storage accounting can report all required categories.
- Migration runner has tests.

---

## 12. Testing Requirements

- Migration from empty database.
- Foreign key enforcement.
- Unique source path constraint.
- File status updates.
- Stale chunk replacement transaction.
- Cache expiration deletion.
- Reset catalog behavior.
- Schema migration rollback behavior on failure.
- Repository-level integration tests using temporary SQLite files.

---

## 13. Unresolved Questions

- Should vectors initially be stored as SQLite BLOBs or external files?
- Should SQLite FTS5 be contentless or external-content?
- Should app settings be JSON values or typed tables?
- Should app_events be rotated or stored outside SQLite?
- Should search history be disabled by default?

---

## 14. Decision

Adopt SQLite via `rusqlite` as the local catalog database.

Treat SQLite as the authoritative local catalog, not merely as a cache.


---

## 15. Amendment: localcache Reference

See `appendices/APPENDIX-A-localcache-integration.md`.

Normative summary:

- `orbok-catalog.sqlite3` remains authoritative.
- `orbok-cache.sqlite3` may be managed by `localcache`.
- `cache_engines` tracks namespace and payload-version metadata only.

---

## 16. Amendment (2026-06-06): rusqlite Version Alignment

`orbok-db` must pin `rusqlite = "0.40"` with the `bundled` feature.

Reason: the adopted cache engine `localcache` v0.19.1 (Appendix A)
depends on `rusqlite` 0.40 (`bundled`), and Cargo permits only one
linked `libsqlite3-sys` version per dependency graph. Any future
`rusqlite` upgrade must be coordinated with a `localcache` upgrade.

The bundled SQLite build enables FTS5, which RFC-007 relies on.
