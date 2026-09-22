-- Task 080 (RFC-005 output, RFC-037 §8, RFC-062 §5 pattern): widen
-- files.file_status's CHECK constraint to admit 'no_text_found'.
--
-- A file orbok can read but that yields no extractable text -- a scanned
-- PDF is the common case -- used to finish every job successfully and stay
-- 'discovered' forever: ChunkAndIndexWorker::run returned early when
-- extraction produced no segments, so the file was never marked indexed,
-- but nothing marked it anything else either. 'discovered' is labelled
-- "Waiting", so orbok had finished with the file and told the user it
-- hadn't started (Review Request 255 §5). A distinct terminal status lets
-- the Folders card and a future query answer "how many files have no
-- text?" from the catalog, not a guess.
--
-- Same table-rewrite SQLite requires for any CHECK change (there is no
-- ALTER TABLE ... ADD/DROP CONSTRAINT), the pattern 0007 established for
-- index_jobs.status. No column is added or removed; every existing row's
-- file_status is preserved unchanged by the plain INSERT...SELECT.

PRAGMA foreign_keys=OFF;

CREATE TABLE files_new (
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
            'deleted', 'permission_denied', 'unsupported', 'failed',
            'no_text_found'
        )
    ),
    last_seen_at TEXT NOT NULL,
    last_scanned_at TEXT,
    last_indexed_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(source_id, canonical_path)
);

INSERT INTO files_new (
    file_id, source_id, original_path, canonical_path, display_path,
    extension, mime_type, file_size_bytes, modified_at, platform_file_key,
    content_hash, hash_algorithm, file_status, last_seen_at,
    last_scanned_at, last_indexed_at, created_at, updated_at
)
SELECT
    file_id, source_id, original_path, canonical_path, display_path,
    extension, mime_type, file_size_bytes, modified_at, platform_file_key,
    content_hash, hash_algorithm, file_status, last_seen_at,
    last_scanned_at, last_indexed_at, created_at, updated_at
FROM files;

DROP TABLE files;
ALTER TABLE files_new RENAME TO files;

CREATE INDEX idx_files_source_id ON files(source_id);
CREATE INDEX idx_files_status ON files(file_status);
CREATE INDEX idx_files_hash ON files(content_hash);
CREATE INDEX idx_files_modified_at ON files(modified_at);

PRAGMA foreign_keys=ON;
