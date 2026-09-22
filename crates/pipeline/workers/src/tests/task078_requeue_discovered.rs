//! Task 078: a `discovered` file with no queued or running extract/chunk
//! job is picked up again at startup, not left behind forever.

use crate::run_startup_recovery;
use orbok_core::{
    FileStatus, HiddenFilePolicy, IndexMode, JobStatus, JobType, PersistenceMode, SourceType,
    SymlinkPolicy,
};
use orbok_db::Catalog;
use orbok_db::repo::{
    FileRepository, IndexJobRepository, NewFile, NewSource, ObservedMetadata, SourceRepository,
};

fn setup(root: &std::path::Path) -> Catalog {
    Catalog::open(root.join("catalog.sqlite3")).unwrap()
}

fn add_source(catalog: &Catalog, root: &std::path::Path) -> orbok_core::SourceId {
    let path = std::fs::canonicalize(root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    SourceRepository::new(catalog)
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
}

fn add_file(
    catalog: &Catalog,
    source_id: &orbok_core::SourceId,
    name: &str,
    status: FileStatus,
) -> orbok_core::FileId {
    FileRepository::new(catalog)
        .insert(NewFile {
            source_id: source_id.clone(),
            original_path: format!("/root/{name}"),
            canonical_path: format!("/root/{name}"),
            display_path: name.to_string(),
            extension: Some("md".into()),
            metadata: ObservedMetadata {
                file_size_bytes: 10,
                modified_at: Some("2026-01-01T00:00:00Z".into()),
                platform_file_key: None,
                content_hash: Some(format!("hash-{name}")),
            },
            status,
        })
        .unwrap()
        .file_id
}

fn active_jobs_for(catalog: &Catalog, file_id: &orbok_core::FileId) -> i64 {
    catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE file_id = ?1 \
             AND job_type IN ('extract', 'chunk') \
             AND status IN ('queued', 'running', 'paused', 'blocked', 'waiting_for_dependency')",
            [file_id.as_str()],
            |r| r.get(0),
        )
        .unwrap()
}

/// Test 1 (red before the fix): a `discovered` file with no jobs stays that
/// way through the two repair steps that already exist -- only the new one
/// queues it.
#[test]
fn a_discovered_file_with_no_job_is_requeued_at_startup() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = setup(dir.path());
    let source_id = add_source(&catalog, dir.path());
    let file_id = add_file(&catalog, &source_id, "stuck.md", FileStatus::Discovered);
    assert_eq!(active_jobs_for(&catalog, &file_id), 0, "no job yet");

    let cache_path = dir.path().join(orbok_db::CACHE_FILE_NAME);
    let report = run_startup_recovery(&catalog, &cache_path).unwrap();

    assert_eq!(
        report.jobs_requeued_discovered, 1,
        "the stuck file must be counted as requeued"
    );
    assert_eq!(
        active_jobs_for(&catalog, &file_id),
        1,
        "the stuck file must now have exactly one extract/chunk job"
    );
    let job_type: String = catalog
        .lock()
        .query_row(
            "SELECT job_type FROM index_jobs WHERE file_id = ?1",
            [file_id.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(job_type, "extract");
}

/// Test 2: an `indexed` file, a `discovered` file that already has a
/// queued job, and a file whose job is `running` are all left with exactly
/// the job set they had (the `running` one is still reset to `queued`, as
/// before this task).
#[test]
fn files_with_a_job_or_that_are_not_discovered_are_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = setup(dir.path());
    let source_id = add_source(&catalog, dir.path());

    let indexed = add_file(&catalog, &source_id, "indexed.md", FileStatus::Indexed);
    let already_queued = add_file(&catalog, &source_id, "queued.md", FileStatus::Discovered);
    let running = add_file(&catalog, &source_id, "running.md", FileStatus::Discovered);

    let jobs = IndexJobRepository::new(&catalog);
    jobs.enqueue(JobType::Extract, Some(&source_id), Some(&already_queued))
        .unwrap();
    let running_job = jobs
        .enqueue(JobType::Extract, Some(&source_id), Some(&running))
        .unwrap();
    jobs.set_status(&running_job, JobStatus::Running).unwrap();

    let cache_path = dir.path().join(orbok_db::CACHE_FILE_NAME);
    let report = run_startup_recovery(&catalog, &cache_path).unwrap();

    assert_eq!(
        report.jobs_requeued_discovered, 0,
        "every discovered file already had a job"
    );
    assert_eq!(active_jobs_for(&catalog, &indexed), 0);
    assert_eq!(
        active_jobs_for(&catalog, &already_queued),
        1,
        "must not gain a second job"
    );
    assert_eq!(
        active_jobs_for(&catalog, &running),
        1,
        "must still have exactly one job"
    );
    assert_eq!(
        report.jobs_reset, 1,
        "the running job is still reset to queued, unchanged from before this task"
    );
    let status: String = catalog
        .lock()
        .query_row(
            "SELECT status FROM index_jobs WHERE job_id = ?1",
            [running_job.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "queued");
}

/// Test 3: running recovery twice queues one extraction, not two.
#[test]
fn running_recovery_twice_queues_the_file_only_once() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = setup(dir.path());
    let source_id = add_source(&catalog, dir.path());
    let file_id = add_file(&catalog, &source_id, "stuck.md", FileStatus::Discovered);

    let cache_path = dir.path().join(orbok_db::CACHE_FILE_NAME);
    let first = run_startup_recovery(&catalog, &cache_path).unwrap();
    let second = run_startup_recovery(&catalog, &cache_path).unwrap();

    assert_eq!(first.jobs_requeued_discovered, 1);
    assert_eq!(
        second.jobs_requeued_discovered, 0,
        "the second run must find the file already has a job"
    );
    assert_eq!(
        active_jobs_for(&catalog, &file_id),
        1,
        "exactly one job must exist after both runs"
    );
}

/// Test 4: the reported count matches the number of files actually queued,
/// across several stuck files at once.
#[test]
fn the_reported_count_matches_the_number_of_files_queued() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = setup(dir.path());
    let source_id = add_source(&catalog, dir.path());
    let stuck: Vec<_> = (0..5)
        .map(|i| {
            add_file(
                &catalog,
                &source_id,
                &format!("stuck{i}.md"),
                FileStatus::Discovered,
            )
        })
        .collect();
    // One that must not be counted.
    add_file(&catalog, &source_id, "ready.md", FileStatus::Indexed);

    let cache_path = dir.path().join(orbok_db::CACHE_FILE_NAME);
    let report = run_startup_recovery(&catalog, &cache_path).unwrap();

    assert_eq!(report.jobs_requeued_discovered, 5);
    for file_id in &stuck {
        assert_eq!(active_jobs_for(&catalog, file_id), 1);
    }
}
