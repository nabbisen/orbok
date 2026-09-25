//! Task 055 §4: what `enqueue_embedding_backfill` costs on a large catalog,
//! since it runs at every startup that has a model.
//!
//! A measurement, not a gate -- `#[ignore]`d so CI never times it. Run:
//! `cargo test -p orbok-db --release --lib task055_backfill_cost -- --ignored --nocapture`
//!
//! The fixture is written with plain SQL (the schema's own required columns),
//! on disk rather than in memory so page-cache behaviour resembles a real
//! profile: 2,000 files x 10 active chunks = 20,000 chunks, each file with
//! one old `failed` embedding job, as the pre-install run leaves behind.

use crate::Catalog;
use crate::repo::IndexJobRepository;
use orbok_core::ModelId;
use std::time::Instant;

const FILES: usize = 2_000;
const CHUNKS_PER_FILE: usize = 10;

fn build(catalog: &Catalog, embedded: bool) -> ModelId {
    let mut conn = catalog.lock();
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let t = "2026-09-16T00:00:00Z";
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
        tx.execute(
            "INSERT INTO index_jobs (job_id, source_id, file_id, job_type, status, \
             error_category, created_at, updated_at) \
             VALUES (?1,'s',?2,'embedding','failed','model_missing',?3,?3)",
            rusqlite::params![format!("j{f}"), file_id, t],
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
            if embedded {
                tx.execute(
                    "INSERT INTO embeddings (embedding_id, chunk_id, model_id, vector_format, \
                     dimension, norm, storage_location, vector_blob, status, created_at, \
                     updated_at) VALUES (?1,?2,'m','fp32',8,'l2','sqlite_blob',?3,'active',?4,?4)",
                    rusqlite::params![format!("e{f}_{c}"), chunk_id, blob, t],
                )
                .unwrap();
            }
        }
    }
    tx.commit().unwrap();
    ModelId::from_string("m".to_string())
}

fn measure(label: &str, embedded: bool) {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(dir.path().join("orbok-catalog.sqlite3")).unwrap();
    let model_id = build(&catalog, embedded);
    let jobs = IndexJobRepository::new(&catalog);
    let start = Instant::now();
    let queued = jobs.enqueue_embedding_backfill(&model_id).unwrap();
    let first = start.elapsed();
    let start = Instant::now();
    let again = jobs.enqueue_embedding_backfill(&model_id).unwrap();
    let second = start.elapsed();
    println!(
        "{label}: {} chunks / {FILES} files -- first call {first:?} (queued {queued}), \
         second call {second:?} (queued {again})",
        FILES * CHUNKS_PER_FILE
    );
}

#[test]
#[ignore = "measurement; run with --ignored --nocapture"]
fn task055_backfill_cost_at_20000_chunks() {
    measure("steady state (every chunk embedded)", true);
    measure("first start after install (no chunk embedded)", false);
}

/// The regression guard for the measurement above, cheap enough for CI: the
/// backfill must reach chunks by file and embeddings by chunk. Through the
/// status indexes the same query took 10.5 s at 20,000 chunks.
#[test]
fn embedding_backfill_uses_the_file_and_chunk_indexes() {
    let catalog = Catalog::open_in_memory().unwrap();
    let conn = catalog.lock();
    let mut stmt = conn
        .prepare(&format!(
            "EXPLAIN QUERY PLAN {}",
            crate::repo::jobs::EMBEDDING_BACKFILL_FILES_SQL
        ))
        .unwrap();
    let plan: Vec<String> = stmt
        .query_map(["m"], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let plan = plan.join("\n");
    for expected in ["idx_chunks_file_id", "idx_embeddings_chunk_id"] {
        assert!(
            plan.contains(expected),
            "expected {expected} in plan:\n{plan}"
        );
    }
    for forbidden in [
        "idx_chunks_status",
        "idx_embeddings_status",
        "idx_embeddings_model_id",
    ] {
        assert!(
            !plan.contains(forbidden),
            "{forbidden} makes the backfill quadratic:\n{plan}"
        );
    }
}
