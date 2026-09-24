//! Task 099 (RFC-011 §14 criteria 5/6): `CleanupExecutor::delete_keyword_index`/
//! `delete_vector_index` delete what they name and nothing else, mark the
//! affected files for rebuild, and the reset delete order stays
//! `sources` before `models` (Review 276 §3).

use crate::Catalog;
use crate::repo::{CleanupExecutor, IndexJobRepository};
use orbok_core::ModelId;

const FILES: usize = 5;
const CHUNKS_PER_FILE: usize = 3;

/// Seeds `FILES` files, each with `CHUNKS_PER_FILE` active chunks, a
/// `keyword_index_records`/`chunk_fts`/`chunk_fts_trigram` row per chunk,
/// and, if `embedded`, an active `embeddings` row per chunk under model
/// `"m"`. Mirrors `task055_backfill_cost.rs`'s fixture shape.
fn build(catalog: &Catalog, embedded: bool) -> ModelId {
    let mut conn = catalog.lock();
    let tx = conn.transaction().unwrap();
    let t = "2026-09-23T00:00:00Z";
    tx.execute(
        "INSERT INTO sources (source_id, source_type, persistence_mode, original_path, \
         canonical_path, status, index_mode, hidden_file_policy, symlink_policy, created_at, \
         updated_at) VALUES ('s','directory','persistent','/d','/d','active','balanced', \
         'exclude','ignore',?1,?1)",
        [t],
    )
    .unwrap();
    tx.execute(
        "INSERT INTO models (model_id, role, model_name, model_version, dimension, status, \
         created_at, updated_at) VALUES ('m','embedding','mock','v1',8,'available',?1,?1)",
        [t],
    )
    .unwrap();
    let blob = vec![0u8; 8 * 4];
    for f in 0..FILES {
        let file_id = format!("f{f}");
        tx.execute(
            "INSERT INTO files (file_id, source_id, original_path, canonical_path, display_path, \
             file_size_bytes, file_status, last_seen_at, created_at, updated_at) \
             VALUES (?1,'s',?2,?2,?2,1,'indexed',?3,?3,?3)",
            rusqlite::params![file_id, format!("/d/{f}.md"), t],
        )
        .unwrap();
        tx.execute(
            "INSERT INTO extraction_records (extraction_id, file_id, extractor_name, \
             extractor_version, normalization_version, status, created_at, updated_at) \
             VALUES (?1,?2,'md','1','1','succeeded',?3,?3)",
            rusqlite::params![format!("x{f}"), file_id, t],
        )
        .unwrap();
        for c in 0..CHUNKS_PER_FILE {
            let chunk_id = format!("c{f}_{c}");
            tx.execute(
                "INSERT INTO chunks (chunk_id, file_id, extraction_id, chunk_kind, chunk_ordinal, \
                 chunk_status, created_at, updated_at) \
                 VALUES (?1,?2,?3,'section',?4,'active',?5,?5)",
                rusqlite::params![chunk_id, file_id, format!("x{f}"), c as i64, t],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO chunk_fts (title, heading_path, normalized_text) \
                 VALUES ('', '', ?1)",
                [format!("text {chunk_id}")],
            )
            .unwrap();
            let fts_rowid = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO keyword_index_records (chunk_id, fts_rowid, index_engine, \
                 tokenizer_name, tokenizer_version, indexed_at, status) \
                 VALUES (?1,?2,'fts5','unicode61','1',?3,'active')",
                rusqlite::params![chunk_id, fts_rowid, t],
            )
            .unwrap();
            if embedded {
                tx.execute(
                    "INSERT INTO embeddings (embedding_id, chunk_id, model_id, vector_format, \
                     dimension, norm, storage_location, vector_blob, status, created_at, \
                     updated_at) VALUES (?1,?2,'m','fp32',8,'l2','sqlite_blob',?3,'active',?4,?4)",
                    rusqlite::params![format!("e{chunk_id}"), chunk_id, blob, t],
                )
                .unwrap();
            }
        }
    }
    tx.commit().unwrap();
    ModelId::from_string("m".to_string())
}

fn table_count(catalog: &Catalog, table: &str) -> i64 {
    catalog
        .lock()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

fn job_count_of_type(catalog: &Catalog, job_type: &str) -> i64 {
    catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE job_type = ?1 AND status = 'queued'",
            [job_type],
            |r| r.get(0),
        )
        .unwrap()
}

/// Every row of `table`, every column, as text -- a snapshot to compare
/// before and after, so "untouched" is asserted on content, not on a count.
fn dump(catalog: &Catalog, table: &str) -> Vec<Vec<String>> {
    let conn = catalog.lock();
    let mut stmt = conn
        .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
        .unwrap();
    let columns = stmt.column_count();
    stmt.query_map([], |row| {
        (0..columns)
            .map(|i| row.get_ref(i).map(|v| format!("{v:?}")))
            .collect::<Result<Vec<_>, _>>()
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}

/// §5 test 1 (keyword half) / §5 test 2: the executor arm exists, deletes
/// the keyword index and nothing else, and queues a rebuild.
#[test]
fn delete_keyword_index_removes_keyword_data_and_keeps_embeddings() {
    let catalog = Catalog::open_in_memory().unwrap();
    build(&catalog, true);
    assert_eq!(
        table_count(&catalog, "keyword_index_records"),
        (FILES * CHUNKS_PER_FILE) as i64
    );
    assert_eq!(
        table_count(&catalog, "embeddings"),
        (FILES * CHUNKS_PER_FILE) as i64
    );

    let sources_before = dump(&catalog, "sources");
    let settings_before = dump(&catalog, "app_settings");
    assert!(!sources_before.is_empty());

    let outcome = CleanupExecutor::new(&catalog)
        .delete_keyword_index()
        .unwrap();

    // §15 test 4: the source settings (and the catalog's own settings) are
    // exactly as they were, column for column.
    assert_eq!(dump(&catalog, "sources"), sources_before);
    assert_eq!(dump(&catalog, "app_settings"), settings_before);

    assert_eq!(table_count(&catalog, "keyword_index_records"), 0);
    assert_eq!(table_count(&catalog, "chunk_fts"), 0);
    assert_eq!(table_count(&catalog, "chunk_fts_trigram"), 0);
    // §2.7: the other index survives untouched.
    assert_eq!(
        table_count(&catalog, "embeddings"),
        (FILES * CHUNKS_PER_FILE) as i64,
        "deleting the keyword index must not touch embeddings"
    );
    // Chunks and files themselves are untouched -- only the derived index.
    assert_eq!(
        table_count(&catalog, "chunks"),
        (FILES * CHUNKS_PER_FILE) as i64
    );
    assert_eq!(table_count(&catalog, "files"), FILES as i64);

    assert_eq!(outcome.files_marked_for_rebuild, FILES as u64);
    assert_eq!(job_count_of_type(&catalog, "extract"), FILES as i64);
}

/// §5 test 1 (meaning half) / §5 test 2: the executor arm exists, deletes
/// the vector index and nothing else, and queues a rebuild.
#[test]
fn delete_vector_index_removes_embeddings_and_keeps_keyword_data() {
    let catalog = Catalog::open_in_memory().unwrap();
    let model_id = build(&catalog, true);
    assert_eq!(
        table_count(&catalog, "embeddings"),
        (FILES * CHUNKS_PER_FILE) as i64
    );
    assert_eq!(
        table_count(&catalog, "keyword_index_records"),
        (FILES * CHUNKS_PER_FILE) as i64
    );

    let sources_before = dump(&catalog, "sources");
    let settings_before = dump(&catalog, "app_settings");
    assert!(!sources_before.is_empty());

    let outcome = CleanupExecutor::new(&catalog)
        .delete_vector_index(Some(&model_id))
        .unwrap();

    // §15 test 3's symmetry for the source settings.
    assert_eq!(dump(&catalog, "sources"), sources_before);
    assert_eq!(dump(&catalog, "app_settings"), settings_before);

    assert_eq!(table_count(&catalog, "embeddings"), 0);
    // §2.7: the other index survives untouched.
    assert_eq!(
        table_count(&catalog, "keyword_index_records"),
        (FILES * CHUNKS_PER_FILE) as i64,
        "deleting the vector index must not touch the keyword index"
    );
    assert_eq!(
        table_count(&catalog, "chunk_fts"),
        (FILES * CHUNKS_PER_FILE) as i64
    );
    assert_eq!(
        table_count(&catalog, "chunks"),
        (FILES * CHUNKS_PER_FILE) as i64
    );
    assert_eq!(table_count(&catalog, "files"), FILES as i64);

    assert_eq!(outcome.files_marked_for_rebuild, FILES as u64);
    assert_eq!(job_count_of_type(&catalog, "embedding"), FILES as i64);
}

/// Stop condition check (§7): with no model configured, the index is
/// still deleted, but nothing is queued -- there is nothing for a queued
/// embedding job to run against, so nothing is marked, rather than
/// leaving files retrying forever.
#[test]
fn delete_vector_index_with_no_model_deletes_but_queues_nothing() {
    let catalog = Catalog::open_in_memory().unwrap();
    build(&catalog, true);

    let outcome = CleanupExecutor::new(&catalog)
        .delete_vector_index(None)
        .unwrap();

    assert_eq!(table_count(&catalog, "embeddings"), 0);
    assert_eq!(outcome.files_marked_for_rebuild, 0);
    assert_eq!(job_count_of_type(&catalog, "embedding"), 0);
}

/// §5 test 2: red today (before this task) -- both actions used to return
/// `CleanupWouldTouchPersistentData` from `run_safe`'s catch-all arm, a
/// misleading error since neither's `affected_classes()` includes
/// `PersistentCatalog`.
#[test]
fn run_safe_no_longer_returns_the_misleading_error_for_either_action() {
    use orbok_core::{CleanupAction, CleanupPlan};

    let catalog = Catalog::open_in_memory().unwrap();
    let model_id = build(&catalog, true);
    let executor = CleanupExecutor::new(&catalog);

    let keyword_plan = CleanupPlan::for_action(CleanupAction::DeleteKeywordIndex, 0);
    executor
        .run_safe(&keyword_plan)
        .expect("DeleteKeywordIndex must have a real executor arm");

    let vector_plan =
        CleanupPlan::for_action(CleanupAction::DeleteVectorIndex, 0).with_model(model_id);
    executor
        .run_safe(&vector_plan)
        .expect("DeleteVectorIndex must have a real executor arm");
}

/// Calling the keyword rebuild twice queues nothing the second time --
/// same idempotence `enqueue_embedding_backfill` already has, since a
/// file with an unfinished extract/chunk job is excluded.
#[test]
fn delete_keyword_index_backfill_is_idempotent() {
    let catalog = Catalog::open_in_memory().unwrap();
    build(&catalog, false);
    let jobs = IndexJobRepository::new(&catalog);

    let first = jobs.enqueue_extraction_backfill().unwrap();
    let second = jobs.enqueue_extraction_backfill().unwrap();

    assert_eq!(first, FILES);
    assert_eq!(
        second, 0,
        "a file with an unfinished extract job must not be re-queued"
    );
}

/// §4 / §5 test 7: the reset's delete order is load-bearing -- `sources`
/// (whose cascade empties `embeddings`) must run before `models`, the
/// non-cascading FK's target. Asserts the order itself, directly, so a
/// future reordering fails this test without needing to run a reset at
/// all.
#[test]
fn sources_is_deleted_before_models() {
    let order = crate::repo::cleanup::RESET_DELETE_ORDER;
    let sources_pos = order.iter().position(|&t| t == "sources");
    let models_pos = order.iter().position(|&t| t == "models");
    assert!(
        sources_pos.is_some(),
        "\"sources\" must be in the delete order"
    );
    assert!(
        models_pos.is_some(),
        "\"models\" must be in the delete order"
    );
    assert!(
        sources_pos < models_pos,
        "\"sources\" must be deleted before \"models\" -- embeddings.model_id has no ON DELETE \
         clause, so a surviving embeddings row referencing a deleted model fails the reset's own \
         transaction"
    );
}

/// The behavioural half of the order check above: with the real schema,
/// reordering `models` before `sources` (while an active embedding still
/// exists) makes the reset's own transaction fail on the FK check, exactly
/// as the comment on `RESET_DELETE_ORDER` says.
#[test]
fn reset_would_fail_if_models_ran_before_sources() {
    let catalog = Catalog::open_in_memory().unwrap();
    build(&catalog, true);
    let mut conn = catalog.lock();
    let tx = conn.transaction().unwrap();
    tx.execute("DELETE FROM models", []).expect_err(
        "deleting models before sources must fail the FK check, proving the order matters",
    );
}
