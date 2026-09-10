//! RFC-059 §7/§11 open question 1: a real measurement pass for the
//! extraction cache's TTL and entry-cap values. No answer exists yet in
//! the RFC; this is the one measurement pass the handoff asks for, not an
//! invented number.
//!
//! Run against a real corpus -- this repository's own `rfcs/` tree (109
//! real markdown documents, ~1.1 MB total as of this writing), not a
//! synthetic fixture. `#[ignore]`d: real indexing work, meant to be run
//! once deliberately, not on every `cargo test`.
//!
//! ```sh
//! cargo test -p orbok --bin orbok --release \
//!   measure_extraction_cache_usage_against_the_rfcs_corpus -- --ignored --nocapture
//! ```

use orbok::runtime_context::{PlatformRuntimePaths, RuntimeContext, RuntimeSelection};
use std::path::Path;
use std::time::Duration;

fn test_context(data_dir: &Path) -> RuntimeContext {
    RuntimeContext::resolve(
        RuntimeSelection::resolve(false, Some(data_dir.as_os_str().to_os_string())).unwrap(),
        data_dir,
        PlatformRuntimePaths {
            standard_data_dir: Some(data_dir),
            standard_settings_dir: Some(data_dir),
        },
    )
    .unwrap()
}

#[tokio::test]
#[ignore = "one-time measurement pass, RFC-059 §7/§11 open question 1 -- read the printed numbers"]
async fn measure_extraction_cache_usage_against_the_rfcs_corpus() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rfcs");
    assert!(
        corpus.is_dir(),
        "expected the workspace's own rfcs/ directory at {corpus:?}"
    );

    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let catalog = crate::bootstrap::open_catalog(&context).unwrap();
    let (card, _) = crate::bootstrap::add_source(&catalog, &corpus.to_string_lossy()).unwrap();
    crate::bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();

    // Drain the hosted scheduler exactly the way the running app does --
    // no embedding model wired in (embedding_parts: None): this
    // measurement is about the extraction cache alone, not the embedding
    // step, and RFC013_MODEL_DIR's own real-model requirement would make
    // this measurement depend on hardware most machines don't have.
    let cache = crate::bootstrap::cache_service(&context).unwrap();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let (_close_tx, resource_signals) =
        futures::channel::mpsc::channel::<crate::scheduler_host::ResourceObservation>(1);
    let loop_catalog = crate::bootstrap::open_catalog(&context).unwrap();
    let handle = tokio::spawn(crate::scheduler_host::run_with_context(
        loop_catalog,
        cache,
        None,
        true,
        true,
        resource_signals,
        tx,
        None,
    ));
    drop(rx); // never drained: sends must fail-fast, not block.

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
        if start.elapsed() > Duration::from_secs(120) {
            handle.abort();
            panic!("indexing the rfcs/ corpus did not finish within 120s");
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

    let usage_cache = crate::bootstrap::cache_service(&context).unwrap();
    let usage = usage_cache
        .usage(
            &catalog,
            &[orbok_cache::OrbokCacheNamespace::ExtractSegments],
        )
        .unwrap();
    let ns = &usage[0];
    let bytes_per_entry = ns.payload_bytes.checked_div(ns.entries).unwrap_or(0);
    println!(
        "RFC-059 §7/§11 open question 1 -- extraction cache usage against rfcs/ \
         ({file_count} real markdown files indexed): {} entries, {} total payload \
         bytes, {bytes_per_entry} bytes/entry average",
        ns.entries, ns.payload_bytes
    );
    assert_eq!(
        ns.entries as i64, file_count,
        "one extraction-cache entry per indexed file"
    );
}
