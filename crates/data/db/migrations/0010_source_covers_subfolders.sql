-- RFC-064 §3.1 (Task 114): whether a folder covers its subfolders.
--
-- `1` (the default, and what every existing folder gets) is "This folder and
-- subfolders": the scanner descends. `0` is "This folder only": the scanner
-- reads the folder's direct entries and does not descend.
--
-- A column rather than a use of `include_patterns_json`: a pattern would be a
-- second way to say the same thing, one that could disagree with the setting
-- the card shows, and RFC-003 Amendment 1 dropped include/exclude patterns as
-- a user-facing feature.
--
-- Added as a new column rather than by editing a released migration
-- (RFC-062 §7). SQLite tests the CHECK against the rows that already exist.
ALTER TABLE sources ADD COLUMN covers_subfolders INTEGER NOT NULL DEFAULT 1
    CHECK (covers_subfolders IN (0, 1));
