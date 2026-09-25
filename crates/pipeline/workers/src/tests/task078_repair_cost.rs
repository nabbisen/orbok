//! Task 078 §2/§5: what the new startup repair step costs.
//!
//! Measurements, not gates -- `#[ignore]`d so CI never times them. Run:
//! `cargo test -p orbok-workers --release --lib task078_repair_cost -- --ignored --nocapture`

use crate::ExtractionWorker;
use crate::run_startup_recovery;
use orbok_cache::CacheService;
use orbok_core::FileId;
use orbok_db::Catalog;
use std::time::Instant;

/// §5 stop condition: does re-checking every `discovered` file at startup
/// cost anything worth noticing when most of them already have a job, as
/// they would mid-way through a large first scan?
///
/// 20,000 files, all `discovered`, all with an active queued `extract` job
/// already -- so every one of them is a no-op for
/// `enqueue_extraction_if_idle`, and the added cost is purely the SELECT
/// plus 20,000 idle-checks that each find a job and do nothing.
#[test]
#[ignore]
fn bulk_requeue_cost_at_20000_discovered_files_already_queued() {
    const FILES: usize = 20_000;
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(dir.path().join("catalog.sqlite3")).unwrap();
    {
        let mut conn = catalog.lock();
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let t = "2026-09-16T00:00:00Z";
        tx.execute(
            "INSERT INTO sources (source_id, source_type, persistence_mode, original_path, \
             canonical_path, status, index_mode, hidden_file_policy, symlink_policy, \
             created_at, updated_at) VALUES ('s','directory','persistent','/d','/d','active', \
             'balanced','exclude','ignore',?1,?1)",
            [t],
        )
        .unwrap();
        for f in 0..FILES {
            let file_id = format!("f{f}");
            tx.execute(
                "INSERT INTO files (file_id, source_id, original_path, canonical_path, \
                 display_path, file_size_bytes, file_status, last_seen_at, created_at, \
                 updated_at) VALUES (?1,'s',?2,?2,?2,1,'discovered',?3,?3,?3)",
                rusqlite::params![file_id, format!("/d/{f}.md"), t],
            )
            .unwrap();
            tx.execute(
                "INSERT INTO index_jobs (job_id, source_id, file_id, job_type, status, \
                 created_at, updated_at) VALUES (?1,'s',?2,'extract','queued',?3,?3)",
                rusqlite::params![format!("j{f}"), file_id, t],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }

    let cache_path = dir.path().join(orbok_db::CACHE_FILE_NAME);
    let start = Instant::now();
    let report = run_startup_recovery(&catalog, &cache_path).unwrap();
    let elapsed = start.elapsed();

    println!(
        "{FILES} discovered files, all already queued: run_startup_recovery took {elapsed:?}, \
         requeued {} (must be 0)",
        report.jobs_requeued_discovered
    );
    assert_eq!(report.jobs_requeued_discovered, 0);
}

/// §2: a file that can never be extracted is re-queued, fails, and is
/// re-queued again at the next startup, forever. What does one such failed
/// attempt cost?
///
/// A `.pdf` file that is not a PDF at all (garbage bytes) -- the closest
/// this suite can build to "a corrupt document" without checking in a
/// binary fixture. Times one real `ExtractionWorker::run` call against it,
/// through the real catalog and cache, the same path a queued job takes.
#[test]
#[ignore]
fn a_permanently_corrupt_files_extraction_attempt_cost() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(dir.path().join("catalog.sqlite3")).unwrap();
    let cache = CacheService::new(dir.path());
    let source_dir = dir.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let bad = source_dir.join("corrupt.pdf");
    // Not a PDF: no header, no xref table -- every real corrupt-file case
    // fails at parse time, before any page is read.
    std::fs::write(&bad, vec![0u8; 4096]).unwrap();

    let source_id = {
        use orbok_core::{HiddenFilePolicy, IndexMode, PersistenceMode, SourceType, SymlinkPolicy};
        use orbok_db::repo::{NewSource, SourceRepository};
        let path = std::fs::canonicalize(&source_dir)
            .unwrap()
            .to_string_lossy()
            .to_string();
        SourceRepository::new(&catalog)
            .insert(NewSource {
                source_type: SourceType::Directory,
                persistence_mode: PersistenceMode::Persistent,
                display_name: None,
                original_path: path.clone(),
                canonical_path: path,
                index_mode: IndexMode::Balanced,
                include_patterns: vec![],
                exclude_patterns: vec![],
                hidden_file_policy: HiddenFilePolicy::Exclude,
                symlink_policy: SymlinkPolicy::Ignore,
                max_file_size_bytes: None,
            })
            .unwrap()
            .source_id
    };
    let file_id = {
        use orbok_core::FileStatus;
        use orbok_db::repo::{FileRepository, NewFile, ObservedMetadata};
        let canonical = std::fs::canonicalize(&bad)
            .unwrap()
            .to_string_lossy()
            .to_string();
        FileRepository::new(&catalog)
            .insert(NewFile {
                source_id: source_id.clone(),
                original_path: canonical.clone(),
                canonical_path: canonical,
                display_path: "corrupt.pdf".into(),
                extension: Some("pdf".into()),
                metadata: ObservedMetadata {
                    file_size_bytes: 4096,
                    modified_at: Some("2026-01-01T00:00:00Z".into()),
                    platform_file_key: None,
                    content_hash: Some("h".into()),
                },
                status: FileStatus::Discovered,
            })
            .unwrap()
            .file_id
    };

    let worker = ExtractionWorker::new(&catalog, &cache);
    let start = Instant::now();
    let result = worker.run(&FileId::from_string(file_id.as_str().to_string()));
    let elapsed = start.elapsed();

    println!(
        "one extraction attempt on a 4 KiB non-PDF '.pdf': {elapsed:?}, result: {}",
        match &result {
            Ok(()) => "Ok (unexpected -- garbage bytes parsed as a PDF)".to_string(),
            Err(e) => format!("Err({e})"),
        }
    );
    assert!(
        result.is_err(),
        "garbage bytes must not parse as a valid PDF"
    );
    // Three such attempts (MAX_JOB_ATTEMPTS) is the worst case per queued
    // chain; this print gives the reviewer the per-attempt figure to
    // multiply.
}
