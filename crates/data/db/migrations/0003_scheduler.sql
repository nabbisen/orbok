-- RFC-036: resource-aware scheduler — new columns on index_jobs.
--
-- NOTE: `priority INTEGER NOT NULL DEFAULT 0` already exists in the
-- baseline (0001) schema. Only the three genuinely new columns are
-- added here. `priority` default is 0 (Maintenance) in the baseline;
-- the application layer maps 2 (NormalBackground) for new jobs.
--
-- attempt_count   : how many times this job has been tried.
-- last_error_kind : most recent error category string for retry context.
-- paused_at       : timestamp when the job was paused by the user.
--
-- New status values ('paused', 'waiting_for_dependency') are in the
-- baseline CHECK constraint for new databases. An upgraded catalog keeps
-- the 0001 baseline's CHECK and rejects these values on INSERT/UPDATE --
-- a stored CHECK, unlike an existing row, IS re-validated going forward.
-- This is a real gap for upgraded catalogs; RFC-062 owns the repair. Do
-- not cite this comment's prior claim (that it isn't a gap) again.
ALTER TABLE index_jobs ADD COLUMN attempt_count   INTEGER NOT NULL DEFAULT 0;
ALTER TABLE index_jobs ADD COLUMN last_error_kind TEXT;
ALTER TABLE index_jobs ADD COLUMN paused_at       TEXT;
