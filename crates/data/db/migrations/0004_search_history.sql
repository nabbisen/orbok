-- RFC-042: search history table.
--
-- Stores recent search instructions (text + filters), not result snapshots.
-- No snippets, embeddings, or ranking data are stored here.
-- Max entries enforced at application layer (default 20).

CREATE TABLE search_history (
    id          TEXT    PRIMARY KEY,
    search_text TEXT    NOT NULL,
    -- JSON array of StoredSearchFilter (serde_json)
    filters_json TEXT   NOT NULL DEFAULT '[]',
    created_at  TEXT    NOT NULL,
    last_used_at TEXT   NOT NULL,
    -- NULL until a result count is known
    result_count INTEGER,
    locale      TEXT    NOT NULL DEFAULT 'en'
);

CREATE INDEX idx_search_history_last_used ON search_history (last_used_at DESC);
