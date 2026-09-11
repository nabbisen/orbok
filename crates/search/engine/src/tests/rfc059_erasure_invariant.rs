//! RFC-059 §6: the erasure invariant, both FTS tables, all five operations
//! that can drop a `keyword_index_records` row (a re-index, a direct
//! keyword-engine delete, `remove_replaced_stale_indexes`, Reset, and
//! Remove folder).
//!
//! ```text
//! count(chunk_fts) == count(keyword_index_records)
//! count(chunk_fts_trigram) == count(keyword_index_records WHERE trigram_fts_rowid IS NOT NULL)
//! ```
//!
//! **This test is the deliverable, not the fixes it exercises** (handoff
//! §2 Slice 2). Confirmed failing on pre-fix code before any of the three
//! original fixes existed: reverting `insert_bundle`'s new pre-insert FTS
//! delete alone made the re-index assertion fail with `chunk_fts` holding
//! one more row than `keyword_index_records` (the orphan the RFC
//! describes), exactly as expected -- restored afterward. Operation 5
//! (Remove folder) was added by RFC-059 Amendment 1 after Review 213 found,
//! by execution, that `SourceRepository::delete_with_all_data` had exactly
//! the same orphaning bug the other three sites had before their own
//! fixes -- this test's own four operations could not see it, because none
//! of them was Remove folder.

use crate::{Fts5KeywordEngine, KeywordSearchEngine};
use orbok_core::{ChunkId, CleanupAction, CleanupPlan, ExtractionId, FileId, SourceId};
use orbok_db::Catalog;
use orbok_db::repo::{ChunkRepository, ChunkSpec, CleanupExecutor, SourceRepository};
use rusqlite::params;

/// Idempotent: safe to call more than once for the same `file_id` (a
/// re-index re-uses the same source/file row, only the extraction is new).
fn ensure_source_and_file(catalog: &Catalog, file_id: &str) -> FileId {
    let conn = catalog.lock();
    let t = "2026-01-01T00:00:00Z";
    conn.execute(
        "INSERT OR IGNORE INTO sources (source_id, source_type, persistence_mode, original_path, \
         canonical_path, status, index_mode, hidden_file_policy, symlink_policy, created_at, \
         updated_at) VALUES ('s1','directory','persistent','/d','/d','active','balanced',\
         'exclude','ignore',?1,?1)",
        params![t],
    )
    .unwrap();
    let path = format!("/d/{file_id}.md");
    conn.execute(
        "INSERT OR IGNORE INTO files (file_id, source_id, original_path, canonical_path, \
         display_path, file_size_bytes, file_status, last_seen_at, created_at, updated_at) \
         VALUES (?1,'s1',?2,?2,?2,1,'indexed',?3,?3,?3)",
        params![file_id, path, t],
    )
    .unwrap();
    FileId::from_string(file_id.to_string())
}

/// A new extraction record for `file_id`. `suffix` disambiguates repeat
/// calls for the same file (a re-index), since `extraction_id` is the
/// table's primary key.
fn seed_extraction(catalog: &Catalog, file_id: &str, suffix: &str) -> ExtractionId {
    let conn = catalog.lock();
    let t = "2026-01-01T00:00:00Z";
    let extraction_id = format!("e-{file_id}-{suffix}");
    conn.execute(
        "INSERT INTO extraction_records (extraction_id, file_id, extractor_name, \
         extractor_version, normalization_version, status, created_at, updated_at) \
         VALUES (?1,?2,'text','v1','norm-v1','succeeded',?3,?3)",
        params![extraction_id, file_id, t],
    )
    .unwrap();
    ExtractionId::from_string(extraction_id)
}

fn seed_source_and_file(catalog: &Catalog, file_id: &str) -> (FileId, ExtractionId) {
    let fid = ensure_source_and_file(catalog, file_id);
    let eid = seed_extraction(catalog, file_id, "1");
    (fid, eid)
}

fn spec(text: &str) -> ChunkSpec {
    ChunkSpec {
        chunk_kind: "paragraph",
        chunk_ordinal: 0,
        heading_path: None,
        title: None,
        normalized_text: text.to_string(),
        line_start: 1,
        line_end: 1,
        byte_start: None,
        byte_end: None,
        location_quality: "exact",
        parent_idx: None,
    }
}

fn chunk_fts_count(catalog: &Catalog) -> i64 {
    catalog
        .lock()
        .query_row("SELECT COUNT(*) FROM chunk_fts", [], |r| r.get(0))
        .unwrap()
}

fn chunk_fts_trigram_count(catalog: &Catalog) -> i64 {
    catalog
        .lock()
        .query_row("SELECT COUNT(*) FROM chunk_fts_trigram", [], |r| r.get(0))
        .unwrap()
}

fn keyword_index_records_count(catalog: &Catalog) -> i64 {
    catalog
        .lock()
        .query_row("SELECT COUNT(*) FROM keyword_index_records", [], |r| {
            r.get(0)
        })
        .unwrap()
}

fn keyword_index_records_with_trigram_count(catalog: &Catalog) -> i64 {
    catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM keyword_index_records WHERE trigram_fts_rowid IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

/// Asserts both RFC-059 §6 invariants, tagging the failure with which
/// operation it was checked after.
fn assert_erasure_invariant(catalog: &Catalog, after: &str) {
    let fts = chunk_fts_count(catalog);
    let records = keyword_index_records_count(catalog);
    assert_eq!(
        fts, records,
        "after {after}: count(chunk_fts)={fts} must equal count(keyword_index_records)={records}"
    );
    let trigram = chunk_fts_trigram_count(catalog);
    let records_with_trigram = keyword_index_records_with_trigram_count(catalog);
    assert_eq!(
        trigram, records_with_trigram,
        "after {after}: count(chunk_fts_trigram)={trigram} must equal \
         count(keyword_index_records WHERE trigram_fts_rowid IS NOT NULL)={records_with_trigram}"
    );
}

#[test]
fn erasure_invariant_holds_after_all_five_operations() {
    let catalog = Catalog::open_in_memory().unwrap();

    // ── Operation 1: a re-index (insert_bundle called twice for the same
    //    file, a different extraction each time -- the real replace path,
    //    RFC-059 §0(i)'s corrected prerequisite). ─────────────────────────
    let (file_id, extraction_1) = seed_source_and_file(&catalog, "f1");
    ChunkRepository::new(&catalog)
        .insert_bundle(&file_id, &extraction_1, &[spec("最初のバージョン")])
        .unwrap();
    assert_erasure_invariant(&catalog, "the first insert");

    let extraction_2 = seed_extraction(&catalog, "f1", "2");
    ChunkRepository::new(&catalog)
        .insert_bundle(&file_id, &extraction_2, &[spec("更新されたバージョン")])
        .unwrap();
    assert_erasure_invariant(&catalog, "a re-index");
    // The count invariant alone does not catch §0(i)'s prerequisite bug:
    // without it, the superseded generation's FTS row is never deleted,
    // but its `keyword_index_records` mapping row survives right along
    // with it (nothing partially deletes either side), so the two counts
    // stay equal to each other while both grow unboundedly -- consistent,
    // just consistently wrong. An absolute count catches what the
    // equality alone cannot: exactly one chunk_fts row must survive a
    // re-index of one chunk, not one per generation ever indexed.
    // Confirmed by reverting only this fix and re-running: chunk_fts held
    // 2 rows here (both generations), and this assertion is what failed --
    // the two-sided invariant above passed throughout, unchanged.
    assert_eq!(
        chunk_fts_count(&catalog),
        1,
        "a re-index must leave exactly the current chunk's chunk_fts row -- \
         the superseded generation's row must be gone, not merely uncounted \
         against"
    );

    // A second file, left untouched by every later operation below --
    // proves each fix is scoped to what it should touch, not a blunt
    // "clear everything" that would pass the invariant vacuously.
    let (control_file, control_extraction) = seed_source_and_file(&catalog, "control");
    ChunkRepository::new(&catalog)
        .insert_bundle(
            &control_file,
            &control_extraction,
            &[spec("変更されない行")],
        )
        .unwrap();
    assert_erasure_invariant(&catalog, "seeding the control file");
    let control_chunk_id = {
        let conn = catalog.lock();
        let id: String = conn
            .query_row(
                "SELECT chunk_id FROM chunks WHERE file_id = ?1",
                params![control_file.as_str()],
                |r| r.get(0),
            )
            .unwrap();
        id
    };

    // ── Operation 2: a direct keyword-engine delete (Fts5KeywordEngine::delete,
    //    RFC-059 §6 item 2 -- no production caller today, per §0(i), but a
    //    public trait method the invariant must hold for regardless). ─────
    let active_chunk_id = {
        let conn = catalog.lock();
        let id: String = conn
            .query_row(
                "SELECT chunk_id FROM chunks WHERE file_id = ?1 AND chunk_status = 'active'",
                params![file_id.as_str()],
                |r| r.get(0),
            )
            .unwrap();
        id
    };
    Fts5KeywordEngine::new(&catalog)
        .delete(&[ChunkId::from_string(active_chunk_id)])
        .unwrap();
    assert_erasure_invariant(&catalog, "Fts5KeywordEngine::delete");

    // ── Operation 3: remove_replaced_stale_indexes (RFC-059 §6 item 3 --
    //    the re-index above left the first generation's chunk 'stale' with
    //    an active replacement, exactly the row this cleanup targets).
    //    Disclosed honestly rather than assumed: in *this* scenario,
    //    `insert_bundle`'s own fix (Operation 1) already deleted that
    //    chunk's FTS rows and its `keyword_index_records` mapping at
    //    replace time, so by the time this call runs there is nothing
    //    left for its own FTS-pre-delete fix to do -- confirmed directly,
    //    not assumed: reverting only `remove_replaced_stale_indexes`'s fix
    //    (keeping `insert_bundle`'s) left this test passing. The
    //    genuinely independent mutation-test coverage for
    //    `remove_replaced_stale_indexes`'s own fix is
    //    `remove_replaced_stale_indexes_deletes_fts_rows_before_the_cascade`
    //    below, which constructs a stale chunk with intact FTS rows
    //    directly (bypassing `insert_bundle`) -- the only way to exercise
    //    that fix in isolation, since every *production* path to a stale
    //    chunk with an active sibling goes through `insert_bundle` first.
    //    Kept here anyway as defense in depth per the handoff's explicit
    //    instruction (§6 item 3): a public cleanup function should not
    //    assume its caller always pre-cleaned. ───────────────────────────
    let stale_before: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE chunk_status = 'stale'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        stale_before > 0,
        "the re-index above must have left a stale chunk for this cleanup to act on"
    );
    let outcome = CleanupExecutor::new(&catalog)
        .run_safe(&CleanupPlan::for_action(
            CleanupAction::RemoveReplacedStaleIndexes,
            0,
        ))
        .unwrap();
    assert!(
        outcome.deleted_rows > 0,
        "remove_replaced_stale_indexes must report rows deleted, not silently reclaim nothing"
    );
    assert_erasure_invariant(&catalog, "remove_replaced_stale_indexes");

    // The control file's chunk must still be there and still searchable --
    // remove_replaced_stale_indexes only ever touches stale/deleted chunks
    // with an active sibling, and the control file has neither.
    let control_survived: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE chunk_id = ?1 AND chunk_status = 'active'",
            params![control_chunk_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        control_survived, 1,
        "the untouched control file's chunk must survive every operation above"
    );

    // ── Operation 4: Reset (run_reset_catalog, RFC-059 §1 Slice 1 -- the
    //    trigram delete-all this handoff adds beside the existing one). ──
    CleanupExecutor::new(&catalog)
        .run_reset_catalog(
            &CleanupPlan::for_action(CleanupAction::ResetCatalog, 0),
            true,
        )
        .unwrap();
    assert_erasure_invariant(&catalog, "Reset");
    assert_eq!(
        chunk_fts_count(&catalog),
        0,
        "Reset must leave chunk_fts empty, including the control file's row"
    );
    assert_eq!(
        chunk_fts_trigram_count(&catalog),
        0,
        "Reset must leave chunk_fts_trigram empty -- the gap this RFC exists to close"
    );

    // ── Operation 5: Remove folder (SourceRepository::delete_with_all_data,
    //    RFC-059 Amendment 1 §2a.1 / criterion 8 -- the fourth erasure site
    //    Review 213 found by execution: this function's own DELETE FROM
    //    sources cascades keyword_index_records away -- the only chunk_id
    //    <-> FTS-rowid link -- before anything deletes the chunk_fts/
    //    chunk_fts_trigram rows that mapping addressed, exactly the shape
    //    the other three sites had before their own fixes. Reset above
    //    emptied every table, so this seeds a fresh source/file rather
    //    than reusing state from Operations 1-4. ─────────────────────────
    let (folder_file_id, folder_extraction) = seed_source_and_file(&catalog, "folder-doc");
    ChunkRepository::new(&catalog)
        .insert_bundle(
            &folder_file_id,
            &folder_extraction,
            &[spec("削除対象のフォルダに含まれる語句")],
        )
        .unwrap();
    assert_erasure_invariant(&catalog, "seeding the folder to be removed");

    let term = "削除対象のフォルダに含まれる語句";
    let trigram_matches_before: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM chunk_fts_trigram WHERE chunk_fts_trigram MATCH ?1",
            params![term],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        trigram_matches_before > 0,
        "the seeded folder's term must be findable in the trigram index before \
         removal, or this test proves nothing"
    );

    SourceRepository::new(&catalog)
        .delete_with_all_data(&SourceId::from_string("s1".to_string()))
        .unwrap();
    assert_erasure_invariant(&catalog, "Remove folder (delete_with_all_data)");

    let trigram_matches_after: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM chunk_fts_trigram WHERE chunk_fts_trigram MATCH ?1",
            params![term],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        trigram_matches_after, 0,
        "Remove folder must leave no trigram match for the removed folder's \
         term, queried against chunk_fts_trigram directly (RFC-059 criterion 8) \
         -- the gap Review 213 found"
    );
    let unicode_matches_after: i64 = catalog
        .lock()
        .query_row("SELECT COUNT(*) FROM chunk_fts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        unicode_matches_after, 0,
        "Remove folder must also leave chunk_fts empty -- both FTS tables, \
         per criterion 8"
    );
}

/// Isolated coverage for `remove_replaced_stale_indexes`'s own FTS-pre-delete
/// fix (RFC-059 §6 item 3), independent of `insert_bundle`'s fix (Operation 1
/// above already cleans up before this cleanup ever sees a stale row in
/// every *production* path -- see that operation's own comment). Constructs
/// a stale chunk with its FTS rows still intact directly, bypassing
/// `insert_bundle` entirely, since that is the only way to put this cleanup
/// in the situation its fix is actually for.
#[test]
fn remove_replaced_stale_indexes_deletes_fts_rows_before_the_cascade() {
    let catalog = Catalog::open_in_memory().unwrap();
    let (file_id, extraction) = seed_source_and_file(&catalog, "f1");

    // The active chunk `remove_replaced_stale_indexes`'s subquery needs to
    // find this file's stale chunk at all.
    ChunkRepository::new(&catalog)
        .insert_bundle(&file_id, &extraction, &[spec("現在のバージョン")])
        .unwrap();
    assert_erasure_invariant(&catalog, "seeding the active chunk");

    // A second chunk, inserted directly as 'active' then flipped to
    // 'stale' by a bare UPDATE -- never touching insert_bundle's replace
    // step, so its FTS rows are never pre-deleted. This is what a stale
    // chunk with an active sibling looks like the moment before
    // `remove_replaced_stale_indexes`'s own fix has ever run.
    let stale_extraction = seed_extraction(&catalog, "f1", "stale");
    let stale_chunk_id = "manually-staled-chunk";
    {
        let conn = catalog.lock();
        let t = "2026-01-01T00:00:00Z";
        conn.execute(
            "INSERT INTO chunks (chunk_id, file_id, extraction_id, chunk_kind, chunk_ordinal, \
             chunk_status, created_at, updated_at) \
             VALUES (?1,?2,?3,'paragraph',0,'active',?4,?4)",
            params![
                stale_chunk_id,
                file_id.as_str(),
                stale_extraction.as_str(),
                t
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chunk_fts (title, heading_path, normalized_text) VALUES (NULL, NULL, '古いバージョン')",
            [],
        )
        .unwrap();
        let fts_rowid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO chunk_fts_trigram (title, heading_path, normalized_text) VALUES (NULL, NULL, '古いバージョン')",
            [],
        )
        .unwrap();
        let trigram_fts_rowid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO keyword_index_records \
             (chunk_id, fts_rowid, trigram_fts_rowid, index_engine, tokenizer_name, \
              tokenizer_version, indexed_at, status) \
             VALUES (?1,?2,?3,'sqlite-fts5','unicode61','chunker-v1',?4,'active')",
            params![stale_chunk_id, fts_rowid, trigram_fts_rowid, t],
        )
        .unwrap();
        // Flip to stale *after* the FTS rows exist -- a bare status
        // update, not insert_bundle's replace step.
        conn.execute(
            "UPDATE chunks SET chunk_status = 'stale' WHERE chunk_id = ?1",
            params![stale_chunk_id],
        )
        .unwrap();
    }
    assert_erasure_invariant(&catalog, "manually staling a chunk with intact FTS rows");
    assert_eq!(
        chunk_fts_count(&catalog),
        2,
        "both the active chunk's and the manually-staled chunk's FTS rows must be present going in"
    );

    let outcome = CleanupExecutor::new(&catalog)
        .run_safe(&CleanupPlan::for_action(
            CleanupAction::RemoveReplacedStaleIndexes,
            0,
        ))
        .unwrap();
    assert!(outcome.deleted_rows > 0);
    // RFC-059 §10 criterion 6: this call actually deleted one chunk_fts
    // row and one chunk_fts_trigram row above, so it must report a
    // non-zero byte reclaim rather than only a row count -- this is the
    // scenario the criterion's "reports a byte reclaim greater than
    // zero" describes, since a normal re-index (see
    // `erasure_invariant_holds_after_all_four_operations`'s own comment
    // on its Operation 3) usually leaves this function nothing to find.
    assert_eq!(
        outcome.bytes_reclaimed, 512,
        "256 bytes per FTS row actually deleted here (one chunk_fts row, \
         one chunk_fts_trigram row)"
    );
    assert_erasure_invariant(
        &catalog,
        "remove_replaced_stale_indexes on an intact-FTS stale chunk",
    );
    assert_eq!(
        chunk_fts_count(&catalog),
        1,
        "only the active chunk's FTS row must survive -- the stale chunk's must be gone, \
         not orphaned by cascading the mapping row out from under it"
    );
    assert_eq!(
        chunk_fts_trigram_count(&catalog),
        1,
        "same for the trigram table"
    );
}
