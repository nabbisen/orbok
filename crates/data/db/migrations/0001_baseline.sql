-- Migration 0001: baseline orbok catalog schema (RFC-002 §7).
-- The catalog is the authoritative local store: persistent catalog data,
-- rebuildable index metadata, ephemeral cache records, storage accounting.
-- localcache payloads live in a separate database (Appendix A §3).

CREATE TABLE app_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE app_settings (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

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
            'discovered', 'indexed', 'stale', 'missing',
            'deleted', 'permission_denied', 'unsupported', 'failed'
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

CREATE TABLE index_jobs (
    job_id TEXT PRIMARY KEY,
    source_id TEXT REFERENCES sources(source_id) ON DELETE CASCADE,
    file_id TEXT REFERENCES files(file_id) ON DELETE CASCADE,
    job_type TEXT NOT NULL CHECK (
        job_type IN (
            'scan', 'extract', 'chunk', 'keyword_index',
            'embedding', 'delete_stale', 'rebuild'
        )
    ),
    status TEXT NOT NULL CHECK (
        status IN ('queued', 'running', 'succeeded', 'failed', 'canceled', 'blocked')
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

CREATE TABLE keyword_index_records (
    chunk_id TEXT PRIMARY KEY REFERENCES chunks(chunk_id) ON DELETE CASCADE,
    -- rowid of the matching chunk_fts row. Contentless FTS5 stores no
    -- column values, so this mapping is the only chunk_id <-> fts link.
    fts_rowid INTEGER,
    index_engine TEXT NOT NULL,
    tokenizer_name TEXT NOT NULL,
    tokenizer_version TEXT NOT NULL,
    language_hint TEXT,
    indexed_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK (
        status IN ('active', 'stale', 'deleted', 'failed')
    )
);

CREATE INDEX idx_keyword_index_status ON keyword_index_records(status);

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

CREATE TABLE storage_accounting (
    category TEXT PRIMARY KEY,
    size_bytes INTEGER NOT NULL,
    item_count INTEGER NOT NULL,
    updated_at TEXT NOT NULL
);

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

CREATE TABLE cache_engines (
    cache_engine_id TEXT PRIMARY KEY,
    engine_kind TEXT NOT NULL CHECK (engine_kind IN ('localcache')),
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

-- Keyword search index (RFC-007 §8.1): contentless FTS5. The index is
-- searchable but stores no retrievable source text; display snippets are
-- loaded dynamically from source files through chunk_locations.
-- contentless_delete=1 (SQLite >= 3.43, available in the bundled build)
-- permits row deletion without retained content.
CREATE VIRTUAL TABLE chunk_fts USING fts5(
    title,
    heading_path,
    normalized_text,
    tokenize = 'unicode61',
    content = '',
    contentless_delete = 1
);
