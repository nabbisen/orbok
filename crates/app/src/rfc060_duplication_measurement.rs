//! RFC-060 §10 / HANDOFF-060 §4 (Slice 5): how often does one file occupy
//! more than one of the top twenty slots?
//!
//! The chunker emits a whole-file `"document"` chunk beside the
//! section-level ones, and both are indexed into both FTS tables; nothing
//! dedupes by `file_id`. The handoff's instruction is **measure before
//! choosing**: if a file rarely appears twice, excluding document chunks
//! from retrieval or capping results per file is not worth the change, and
//! saying so with the number is the right outcome.
//!
//! Run against a real corpus -- this repository's own `rfcs/` tree, not a
//! synthetic fixture. `#[ignore]`d: real indexing work, run deliberately.
//!
//! ```sh
//! cargo test -p orbok --bin orbok --release \
//!   measure_document_chunk_duplication_against_the_rfcs_corpus -- --ignored --nocapture
//! ```

use orbok::runtime_context::{PlatformRuntimePaths, RuntimeContext, RuntimeSelection};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

fn test_context(data_dir: &Path) -> RuntimeContext {
    RuntimeContext::resolve(
        RuntimeSelection::resolve(false, Some(data_dir.as_os_str().to_os_string())).unwrap(),
        data_dir,
        PlatformRuntimePaths {
            standard_data_dir: Some(data_dir),
            standard_settings_dir: Some(data_dir),
            home_dir: None,
        },
    )
    .unwrap()
}

/// Queries drawn from this corpus's own vocabulary -- terms a reader would
/// actually search for in an RFC tree, not words chosen to provoke the
/// defect.
const QUERIES: &[&str] = &[
    "extraction cache",
    "trigram",
    "scheduler",
    "snippet",
    "migration",
    "embedding model",
    "portable mode",
    "acceptance criteria",
    "closure record",
    "keyword search",
    "privacy",
    "benchmark",
    "japanese",
    "pause",
    "wizard",
];

#[tokio::test]
#[ignore = "one-time measurement pass, HANDOFF-060 §4 -- read the printed numbers"]
async fn measure_document_chunk_duplication_against_the_rfcs_corpus() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rfcs");
    assert!(
        corpus.is_dir(),
        "expected the workspace's own rfcs/ directory at {corpus:?}"
    );

    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let catalog = crate::bootstrap::open_catalog(&context).unwrap();
    let (card, _) =
        crate::bootstrap::add_source_expect_added(&catalog, &corpus.to_string_lossy()).unwrap();
    crate::bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();

    let cache = crate::bootstrap::cache_service(&context).unwrap();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let (_close_tx, resource_signals) =
        futures::channel::mpsc::channel::<crate::scheduler_host::ResourceObservation>(1);
    let loop_catalog = crate::bootstrap::open_catalog(&context).unwrap();
    let handle = tokio::spawn(crate::scheduler_host::run_with_context(
        loop_catalog,
        cache,
        crate::scheduler_host::EmbeddingSource::fixed(None),
        true,
        true,
        resource_signals,
        tx,
        None,
    ));
    drop(rx);

    let start = std::time::Instant::now();
    loop {
        let queued: i64 = catalog
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM index_jobs WHERE status IN ('queued','running')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        if queued == 0 {
            break;
        }
        if start.elapsed() > Duration::from_secs(180) {
            handle.abort();
            panic!("indexing the rfcs/ corpus did not finish within 180s");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    handle.abort();

    let file_count: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM files WHERE file_status = 'indexed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let (document_chunks, other_chunks): (i64, i64) = catalog
        .lock()
        .query_row(
            "SELECT \
               SUM(CASE WHEN chunk_kind = 'document' THEN 1 ELSE 0 END), \
               SUM(CASE WHEN chunk_kind != 'document' THEN 1 ELSE 0 END) \
             FROM chunks WHERE chunk_status = 'active'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();

    // No cache handle: this pass counts *slots*, not snippets, and the
    // keyword path decides which chunks take them.
    let mut queries_with_a_repeat = 0usize;
    let mut total_duplicate_slots = 0usize;
    let mut total_results = 0usize;
    let mut per_query = Vec::new();

    // Through the engine rather than `run_search`, because the choice the
    // handoff poses -- exclude `chunk_kind = 'document'`, or cap results
    // per file -- turns on *which* chunks take the slots, and the UI
    // result type does not carry `chunk_id`.
    let mut document_chunk_results = 0usize;
    let mut widest_single_file = 0usize;
    for query in QUERIES {
        let results = orbok_search::HybridSearchService::keyword_only(&catalog)
            .search(query, orbok_search::SearchMode::Auto, 20)
            .unwrap();
        let mut seen: HashMap<String, usize> = HashMap::new();
        for result in &results {
            *seen.entry(result.canonical_path.clone()).or_default() += 1;
            let kind: String = catalog
                .lock()
                .query_row(
                    "SELECT chunk_kind FROM chunks WHERE chunk_id = ?1",
                    rusqlite::params![result.chunk_id.as_str()],
                    |r| r.get(0),
                )
                .unwrap();
            if kind == "document" {
                document_chunk_results += 1;
            }
        }
        widest_single_file = widest_single_file.max(seen.values().copied().max().unwrap_or(0));
        // Slots a repeat file took beyond its first: the cost the handoff
        // asks about, in slots out of twenty.
        let duplicate_slots: usize = seen.values().map(|count| count.saturating_sub(1)).sum();
        let repeated_files = seen.values().filter(|count| **count > 1).count();
        if repeated_files > 0 {
            queries_with_a_repeat += 1;
        }
        total_duplicate_slots += duplicate_slots;
        total_results += results.len();
        per_query.push((*query, results.len(), repeated_files, duplicate_slots));
    }

    println!("\n=== HANDOFF-060 §4: document-chunk duplication, rfcs/ corpus ===");
    println!(
        "corpus: {file_count} files indexed; {document_chunks} document chunks, \
         {other_chunks} section chunks"
    );
    println!(
        "{:<22} {:>7} {:>9} {:>10}",
        "query", "results", "repeated", "dup slots"
    );
    for (query, results, repeated, slots) in &per_query {
        println!("{query:<22} {results:>7} {repeated:>9} {slots:>10}");
    }
    println!(
        "\n{queries_with_a_repeat} of {} queries had at least one file twice; \
         {total_duplicate_slots} duplicate slots out of {total_results} returned results",
        QUERIES.len()
    );
    println!(
        "{document_chunk_results} of {total_results} results were the whole-file \
         'document' chunk; the widest single file took {widest_single_file} of 20 slots"
    );
}
