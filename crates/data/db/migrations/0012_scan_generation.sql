-- Task 116: whether a file was seen by *this scan* is an event, not a time.
--
-- Each scan of a folder takes the next number in `sources.scan_generation` when
-- it starts. Every file the scan sees records that number in
-- `files.seen_generation`. At the end of the scan, a file whose number is older
-- than the folder's is one the scan did not see: `mark_missing_unseen` marks it
-- missing. Before, that was `last_seen_at < scan_started_at`, a comparison of two
-- wall-clock readings, which a clock that steps backwards (NTP, sleep and
-- resume) gets wrong. `last_seen_at` stays as a record, for display; it no longer
-- decides anything.
--
-- Existing rows start at 0 in both columns. The next scan makes the folder's
-- number 1, stamps every file it sees with 1, and marks the rest missing, as
-- before.
ALTER TABLE sources ADD COLUMN scan_generation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE files ADD COLUMN seen_generation INTEGER NOT NULL DEFAULT 0;
