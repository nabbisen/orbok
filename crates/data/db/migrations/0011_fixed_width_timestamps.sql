-- Task 116: every stored timestamp is fixed width.
--
-- `now_iso8601()` / `system_time_iso8601()` now write RFC 3339 UTC with exactly
-- nine fractional digits and `Z` (`2026-09-24T15:26:29.123400000Z`, 30
-- characters). Before, the fraction was written with trailing zeros trimmed and
-- not at all when zero, so widths varied and text comparison (SQLite's) could
-- order a later moment before an earlier one. This rewrites every value already
-- stored to the same width, so old and new values compare correctly with each
-- other.
--
-- * no fraction         -> `.000000000` is inserted before the `Z`;
-- * a shorter fraction  -> it is right-padded with zeros to nine digits;
-- * already nine digits, a NULL, or any value that is not of this shape (none
--   is written by orbok) -> left alone. So it is idempotent.
--
-- `files.modified_at` is compared for *equality* by change detection: without
-- this rewrite every file would look modified on the first scan after upgrade
-- and the whole corpus would be hashed again.
--
-- Every `*_at` column in the schema is listed below (and
-- `schema_migrations.applied_at`, created by the migration runner). Columns that
-- are not text timestamps (`validated_startup_epoch` is an integer epoch
-- counter) are not touched.

UPDATE app_events SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END;

UPDATE app_metadata SET
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE app_settings SET
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE cache_engines SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE chunk_locations SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE chunks SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE embeddings SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE extraction_records SET
    completed_at = CASE
            WHEN completed_at GLOB '????-??-??T??:??:??Z'
                THEN substr(completed_at, 1, 19) || '.000000000Z'
            WHEN completed_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(completed_at, 21, length(completed_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(completed_at) BETWEEN 22 AND 30
                THEN substr(completed_at, 1, 20)
                     || substr(substr(completed_at, 21, length(completed_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE completed_at
        END,
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    started_at = CASE
            WHEN started_at GLOB '????-??-??T??:??:??Z'
                THEN substr(started_at, 1, 19) || '.000000000Z'
            WHEN started_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(started_at, 21, length(started_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(started_at) BETWEEN 22 AND 30
                THEN substr(started_at, 1, 20)
                     || substr(substr(started_at, 21, length(started_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE started_at
        END,
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE files SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    last_indexed_at = CASE
            WHEN last_indexed_at GLOB '????-??-??T??:??:??Z'
                THEN substr(last_indexed_at, 1, 19) || '.000000000Z'
            WHEN last_indexed_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(last_indexed_at, 21, length(last_indexed_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(last_indexed_at) BETWEEN 22 AND 30
                THEN substr(last_indexed_at, 1, 20)
                     || substr(substr(last_indexed_at, 21, length(last_indexed_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE last_indexed_at
        END,
    last_scanned_at = CASE
            WHEN last_scanned_at GLOB '????-??-??T??:??:??Z'
                THEN substr(last_scanned_at, 1, 19) || '.000000000Z'
            WHEN last_scanned_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(last_scanned_at, 21, length(last_scanned_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(last_scanned_at) BETWEEN 22 AND 30
                THEN substr(last_scanned_at, 1, 20)
                     || substr(substr(last_scanned_at, 21, length(last_scanned_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE last_scanned_at
        END,
    last_seen_at = CASE
            WHEN last_seen_at GLOB '????-??-??T??:??:??Z'
                THEN substr(last_seen_at, 1, 19) || '.000000000Z'
            WHEN last_seen_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(last_seen_at, 21, length(last_seen_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(last_seen_at) BETWEEN 22 AND 30
                THEN substr(last_seen_at, 1, 20)
                     || substr(substr(last_seen_at, 21, length(last_seen_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE last_seen_at
        END,
    modified_at = CASE
            WHEN modified_at GLOB '????-??-??T??:??:??Z'
                THEN substr(modified_at, 1, 19) || '.000000000Z'
            WHEN modified_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(modified_at, 21, length(modified_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(modified_at) BETWEEN 22 AND 30
                THEN substr(modified_at, 1, 20)
                     || substr(substr(modified_at, 21, length(modified_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE modified_at
        END,
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE index_jobs SET
    completed_at = CASE
            WHEN completed_at GLOB '????-??-??T??:??:??Z'
                THEN substr(completed_at, 1, 19) || '.000000000Z'
            WHEN completed_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(completed_at, 21, length(completed_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(completed_at) BETWEEN 22 AND 30
                THEN substr(completed_at, 1, 20)
                     || substr(substr(completed_at, 21, length(completed_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE completed_at
        END,
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    paused_at = CASE
            WHEN paused_at GLOB '????-??-??T??:??:??Z'
                THEN substr(paused_at, 1, 19) || '.000000000Z'
            WHEN paused_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(paused_at, 21, length(paused_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(paused_at) BETWEEN 22 AND 30
                THEN substr(paused_at, 1, 20)
                     || substr(substr(paused_at, 21, length(paused_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE paused_at
        END,
    started_at = CASE
            WHEN started_at GLOB '????-??-??T??:??:??Z'
                THEN substr(started_at, 1, 19) || '.000000000Z'
            WHEN started_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(started_at, 21, length(started_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(started_at) BETWEEN 22 AND 30
                THEN substr(started_at, 1, 20)
                     || substr(substr(started_at, 21, length(started_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE started_at
        END,
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE keyword_index_records SET
    indexed_at = CASE
            WHEN indexed_at GLOB '????-??-??T??:??:??Z'
                THEN substr(indexed_at, 1, 19) || '.000000000Z'
            WHEN indexed_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(indexed_at, 21, length(indexed_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(indexed_at) BETWEEN 22 AND 30
                THEN substr(indexed_at, 1, 20)
                     || substr(substr(indexed_at, 21, length(indexed_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE indexed_at
        END;

UPDATE managed_model_generations SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE managed_model_profiles SET
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE models SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    last_validated_at = CASE
            WHEN last_validated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(last_validated_at, 1, 19) || '.000000000Z'
            WHEN last_validated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(last_validated_at, 21, length(last_validated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(last_validated_at) BETWEEN 22 AND 30
                THEN substr(last_validated_at, 1, 20)
                     || substr(substr(last_validated_at, 21, length(last_validated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE last_validated_at
        END,
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE schema_migrations SET
    applied_at = CASE
            WHEN applied_at GLOB '????-??-??T??:??:??Z'
                THEN substr(applied_at, 1, 19) || '.000000000Z'
            WHEN applied_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(applied_at, 21, length(applied_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(applied_at) BETWEEN 22 AND 30
                THEN substr(applied_at, 1, 20)
                     || substr(substr(applied_at, 21, length(applied_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE applied_at
        END;

UPDATE search_history SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    last_used_at = CASE
            WHEN last_used_at GLOB '????-??-??T??:??:??Z'
                THEN substr(last_used_at, 1, 19) || '.000000000Z'
            WHEN last_used_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(last_used_at, 21, length(last_used_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(last_used_at) BETWEEN 22 AND 30
                THEN substr(last_used_at, 1, 20)
                     || substr(substr(last_used_at, 21, length(last_used_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE last_used_at
        END;

UPDATE search_queries SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    expires_at = CASE
            WHEN expires_at GLOB '????-??-??T??:??:??Z'
                THEN substr(expires_at, 1, 19) || '.000000000Z'
            WHEN expires_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(expires_at, 21, length(expires_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(expires_at) BETWEEN 22 AND 30
                THEN substr(expires_at, 1, 20)
                     || substr(substr(expires_at, 21, length(expires_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE expires_at
        END;

UPDATE search_result_cache SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    expires_at = CASE
            WHEN expires_at GLOB '????-??-??T??:??:??Z'
                THEN substr(expires_at, 1, 19) || '.000000000Z'
            WHEN expires_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(expires_at, 21, length(expires_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(expires_at) BETWEEN 22 AND 30
                THEN substr(expires_at, 1, 20)
                     || substr(substr(expires_at, 21, length(expires_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE expires_at
        END,
    last_accessed_at = CASE
            WHEN last_accessed_at GLOB '????-??-??T??:??:??Z'
                THEN substr(last_accessed_at, 1, 19) || '.000000000Z'
            WHEN last_accessed_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(last_accessed_at, 21, length(last_accessed_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(last_accessed_at) BETWEEN 22 AND 30
                THEN substr(last_accessed_at, 1, 20)
                     || substr(substr(last_accessed_at, 21, length(last_accessed_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE last_accessed_at
        END;

UPDATE snippet_cache SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    expires_at = CASE
            WHEN expires_at GLOB '????-??-??T??:??:??Z'
                THEN substr(expires_at, 1, 19) || '.000000000Z'
            WHEN expires_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(expires_at, 21, length(expires_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(expires_at) BETWEEN 22 AND 30
                THEN substr(expires_at, 1, 20)
                     || substr(substr(expires_at, 21, length(expires_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE expires_at
        END,
    last_accessed_at = CASE
            WHEN last_accessed_at GLOB '????-??-??T??:??:??Z'
                THEN substr(last_accessed_at, 1, 19) || '.000000000Z'
            WHEN last_accessed_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(last_accessed_at, 21, length(last_accessed_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(last_accessed_at) BETWEEN 22 AND 30
                THEN substr(last_accessed_at, 1, 20)
                     || substr(substr(last_accessed_at, 21, length(last_accessed_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE last_accessed_at
        END;

UPDATE sources SET
    created_at = CASE
            WHEN created_at GLOB '????-??-??T??:??:??Z'
                THEN substr(created_at, 1, 19) || '.000000000Z'
            WHEN created_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(created_at, 21, length(created_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(created_at) BETWEEN 22 AND 30
                THEN substr(created_at, 1, 20)
                     || substr(substr(created_at, 21, length(created_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE created_at
        END,
    last_scanned_at = CASE
            WHEN last_scanned_at GLOB '????-??-??T??:??:??Z'
                THEN substr(last_scanned_at, 1, 19) || '.000000000Z'
            WHEN last_scanned_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(last_scanned_at, 21, length(last_scanned_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(last_scanned_at) BETWEEN 22 AND 30
                THEN substr(last_scanned_at, 1, 20)
                     || substr(substr(last_scanned_at, 21, length(last_scanned_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE last_scanned_at
        END,
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

UPDATE storage_accounting SET
    updated_at = CASE
            WHEN updated_at GLOB '????-??-??T??:??:??Z'
                THEN substr(updated_at, 1, 19) || '.000000000Z'
            WHEN updated_at GLOB '????-??-??T??:??:??.*Z'
                 AND substr(updated_at, 21, length(updated_at) - 21) NOT GLOB '*[^0-9]*'
                 AND length(updated_at) BETWEEN 22 AND 30
                THEN substr(updated_at, 1, 20)
                     || substr(substr(updated_at, 21, length(updated_at) - 21) || '000000000', 1, 9) || 'Z'
            ELSE updated_at
        END;

