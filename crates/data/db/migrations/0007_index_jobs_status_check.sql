-- RFC-062 §5 step 1: repair index_jobs's status CHECK constraint.
--
-- 0001_baseline.sql shipped in 0.16.0 with
--   status IN ('queued','running','succeeded','failed','canceled','blocked')
-- Commit c54e89d (RFC-036, released in 0.17.0) widened that line in place,
-- in the already-released baseline file, to add 'paused' and
-- 'waiting_for_dependency' -- with no migration to rebuild the table. A
-- catalog created by orbok <= 0.16.0 keeps the CHECK it was created with
-- (SQLite enforces a stored CHECK on every INSERT/UPDATE, even though it
-- does not re-validate existing rows against a changed one) and so rejects
-- status='paused' forever, silently: `scheduler_host.rs`'s
-- `let _ = scheduler.pause(&catalog)` (RFC-061 §8(a) fixed the swallowing,
-- not this) dropped the resulting error, so toggling background indexing
-- off saved the setting and paused nothing.
--
-- This migration rebuilds index_jobs with the full, correct CHECK, using
-- SQLite's standard table-rewrite (there is no ALTER TABLE ... ADD/DROP
-- CONSTRAINT). On a catalog created after c54e89d, the wide CHECK is
-- already in place and this is a no-op in behavior -- it is not detected
-- or skipped as a special case, since re-running it changes nothing.
--
-- 0001_baseline.sql itself is restored to its released 0.16.0 text in the
-- very next migration file (RFC-062 §5 step 2) -- after this one, so
-- fresh installs get the narrow CHECK from 0001 and the wide one from
-- this migration, exactly as an append-only migration list is supposed to
-- work.

PRAGMA foreign_keys=OFF;

CREATE TABLE index_jobs_new (
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
        status IN ('queued', 'running', 'succeeded', 'failed', 'canceled', 'blocked', 'paused', 'waiting_for_dependency')
    ),
    priority INTEGER NOT NULL DEFAULT 0,
    progress_current INTEGER NOT NULL DEFAULT 0,
    progress_total INTEGER,
    error_category TEXT,
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error_kind TEXT,
    paused_at TEXT
);

INSERT INTO index_jobs_new (
    job_id, source_id, file_id, job_type, status, priority,
    progress_current, progress_total, error_category, error_message,
    created_at, updated_at, started_at, completed_at,
    attempt_count, last_error_kind, paused_at
)
SELECT
    job_id, source_id, file_id, job_type, status, priority,
    progress_current, progress_total, error_category, error_message,
    created_at, updated_at, started_at, completed_at,
    attempt_count, last_error_kind, paused_at
FROM index_jobs;

DROP TABLE index_jobs;
ALTER TABLE index_jobs_new RENAME TO index_jobs;

CREATE INDEX idx_index_jobs_status ON index_jobs(status);
CREATE INDEX idx_index_jobs_file_id ON index_jobs(file_id);
CREATE INDEX idx_index_jobs_source_id ON index_jobs(source_id);

PRAGMA foreign_keys=ON;
