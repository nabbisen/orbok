-- Migration 0005: keyword FTS rowid join indexes.
--
-- Search queries join contentless FTS rows back to keyword_index_records through
-- fts_rowid / trigram_fts_rowid. These are lookup keys, not just metadata, so
-- index them to keep candidate retrieval stable as corpus size grows.

CREATE INDEX IF NOT EXISTS idx_keyword_index_fts_rowid
ON keyword_index_records(fts_rowid);

CREATE INDEX IF NOT EXISTS idx_keyword_index_trigram_fts_rowid
ON keyword_index_records(trigram_fts_rowid);
