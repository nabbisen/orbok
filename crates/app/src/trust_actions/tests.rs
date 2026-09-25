//! HANDOFF-038 Slice 2: Prepare again and Check folder, against a real
//! profile and catalog.

use super::recover;
use crate::bootstrap;
use orbok::runtime_context::{PlatformRuntimePaths, RuntimeContext, RuntimeSelection};
use orbok_core::SourceStatus;
use orbok_db::Catalog;
use orbok_db::repo::FileRepository;
use orbok_fs::{ScanRequest, Scanner};
use orbok_search::{ResultRecoveryAction as Action, ResultTrustState as Trust};
use orbok_ui::AppState;
use orbok_ui::notice::UserNotice;
use orbok_ui::state::{Message, ResultTrustDisplay, SearchResultDisplay};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

struct Profile {
    _temp: tempfile::TempDir,
    root: PathBuf,
    db: PathBuf,
    catalog: Catalog,
    source_id: String,
    file_path: String,
}

impl Profile {
    /// A real catalog with one folder holding `doc.md`, scanned but with no
    /// jobs queued (`enqueue_index_jobs: false`), so the tests own the queue.
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("profile");
        let context = RuntimeContext::resolve(
            RuntimeSelection::resolve(false, Some(root.as_os_str().to_os_string())).unwrap(),
            temp.path(),
            PlatformRuntimePaths {
                standard_data_dir: Some(temp.path()),
                standard_settings_dir: Some(temp.path()),
                home_dir: None,
            },
        )
        .unwrap();
        let catalog = bootstrap::open_catalog(&context).unwrap();
        catalog
            .lock()
            .busy_timeout(std::time::Duration::from_millis(50))
            .unwrap();
        let folder = temp.path().join("Docs");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("doc.md"), "# Doc\n\nsomething\n").unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &folder.to_string_lossy()).unwrap();
        let source_id = orbok_core::SourceId::from_string(card.source_id.clone());
        Scanner::new(&catalog)
            .scan(
                &ScanRequest {
                    source_id: source_id.clone(),
                    force_hash: false,
                    enqueue_index_jobs: false,
                },
                &AtomicBool::new(false),
            )
            .unwrap();
        let file_path = std::fs::canonicalize(folder.join("doc.md"))
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(
            FileRepository::new(&catalog)
                .find_by_canonical_path(&file_path)
                .unwrap()
                .is_some(),
            "the scan registered the file"
        );
        Self {
            db: root.join(orbok_db::CATALOG_FILE_NAME),
            root: temp.path().to_path_buf(),
            _temp: temp,
            catalog,
            source_id: card.source_id,
            file_path,
        }
    }

    fn state_with_result(&self, path: &str, trust: Trust) -> AppState {
        let mut state = AppState::default();
        state.update(&Message::SourcesLoaded(
            bootstrap::get_sources(&self.catalog).unwrap(),
        ));
        state.update(&Message::SearchResultsReady(vec![SearchResultDisplay {
            display_path: "doc.md".into(),
            canonical_path: path.into(),
            title: None,
            heading_path: None,
            snippet: None,
            keyword_rank: 1,
            badges: vec![],
            trust: ResultTrustDisplay {
                state: trust,
                recovery_actions: vec![Action::PrepareAgain, Action::CheckFolder],
                warnings: vec![],
            },
        }]));
        state
    }

    fn jobs(&self, job_type: &str) -> i64 {
        self.catalog
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM index_jobs WHERE job_type = ?1 AND status = 'queued'",
                [job_type],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn source_status(&self) -> String {
        self.catalog
            .lock()
            .query_row(
                "SELECT status FROM sources WHERE source_id = ?1",
                [&self.source_id],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn write_lock(&self) -> rusqlite::Connection {
        let locker = rusqlite::Connection::open(&self.db).unwrap();
        locker.execute_batch("BEGIN EXCLUSIVE;").unwrap();
        locker
    }
}

fn message(action: Action) -> Message {
    Message::TrustRecoveryAction {
        result_idx: 0,
        action,
    }
}

fn retry(state: &AppState) -> Option<Message> {
    state.notice_action.as_deref().cloned()
}

// ── Prepare again ────────────────────────────────────────────────────────

/// Prepare again queues one `extract` job for that file, through the job
/// path the scanner uses for a changed file, and the row says so. A second
/// press queues nothing more.
#[test]
fn prepare_again_queues_one_extract_job_for_that_file_and_relabels_the_row() {
    let profile = Profile::new();
    let mut state = profile.state_with_result(&profile.file_path, Trust::NeedsUpdate);
    assert_eq!(profile.jobs("extract"), 0, "control: nothing queued yet");

    recover(
        &profile.catalog,
        &mut state,
        0,
        Action::PrepareAgain,
        &message(Action::PrepareAgain),
    );

    assert_eq!(profile.jobs("extract"), 1, "one extract job");
    let (job_file, job_source): (String, String) = profile
        .catalog
        .lock()
        .query_row(
            "SELECT file_id, source_id FROM index_jobs WHERE job_type = 'extract'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let file = FileRepository::new(&profile.catalog)
        .find_by_canonical_path(&profile.file_path)
        .unwrap()
        .unwrap();
    assert_eq!(job_file, file.file_id.as_str(), "for that file");
    assert_eq!(job_source, profile.source_id, "under that file's folder");
    assert_eq!(
        state.search_results[0].trust.state,
        Trust::StillBeingPrepared
    );
    assert_eq!(state.notice, None);

    recover(
        &profile.catalog,
        &mut state,
        0,
        Action::PrepareAgain,
        &message(Action::PrepareAgain),
    );
    assert_eq!(
        profile.jobs("extract"),
        1,
        "a second press queues nothing more"
    );
}

#[test]
fn prepare_again_for_a_file_the_catalog_does_not_know_says_so() {
    let profile = Profile::new();
    let mut state = profile.state_with_result("/nowhere/ghost.md", Trust::NeedsUpdate);

    recover(
        &profile.catalog,
        &mut state,
        0,
        Action::PrepareAgain,
        &message(Action::PrepareAgain),
    );

    assert_eq!(state.notice, Some(UserNotice::FileCouldNotBeFound));
    assert_eq!(profile.jobs("extract"), 0);
    assert_eq!(
        state.search_results[0].trust.state,
        Trust::NeedsUpdate,
        "the row is unchanged"
    );
}

/// A write lock is a real failure to queue. The notice is the storage one,
/// and its Try again is the same action.
#[test]
fn prepare_again_that_cannot_write_says_so_and_retries_the_same_action() {
    let profile = Profile::new();
    let mut state = profile.state_with_result(&profile.file_path, Trust::NeedsUpdate);
    let _lock = profile.write_lock();

    recover(
        &profile.catalog,
        &mut state,
        0,
        Action::PrepareAgain,
        &message(Action::PrepareAgain),
    );
    drop(_lock);

    assert_eq!(state.notice, Some(UserNotice::StorageUnavailable));
    assert!(matches!(
        retry(&state),
        Some(Message::TrustRecoveryAction {
            result_idx: 0,
            action: Action::PrepareAgain
        })
    ));
    assert_eq!(profile.jobs("extract"), 0);
    assert_eq!(state.search_results[0].trust.state, Trust::NeedsUpdate);
}

// ── Check folder ─────────────────────────────────────────────────────────

/// A reachable folder: the existing source check enqueues a scan.
#[test]
fn check_folder_on_a_reachable_folder_queues_a_scan() {
    let profile = Profile::new();
    let mut state = profile.state_with_result(&profile.file_path, Trust::FileNotFound);
    assert_eq!(profile.jobs("scan"), 0);

    recover(
        &profile.catalog,
        &mut state,
        0,
        Action::CheckFolder,
        &message(Action::CheckFolder),
    );

    assert_eq!(profile.jobs("scan"), 1, "the refresh path's scan job");
    assert_eq!(profile.source_status(), "active");
    assert_eq!(state.notice, None);
}

/// A folder that is gone: the check marks that result's folder missing, and
/// the Folders list follows.
#[test]
fn check_folder_on_a_missing_folder_marks_it_missing() {
    let profile = Profile::new();
    let mut state = profile.state_with_result(&profile.file_path, Trust::FileNotFound);
    std::fs::rename(profile.root.join("Docs"), profile.root.join("Docs-away")).unwrap();

    recover(
        &profile.catalog,
        &mut state,
        0,
        Action::CheckFolder,
        &message(Action::CheckFolder),
    );

    assert_eq!(profile.source_status(), "missing");
    assert_eq!(
        state.sources[0].status,
        SourceStatus::Missing,
        "the list follows the catalog"
    );
}

#[test]
fn check_folder_that_cannot_write_says_so_and_retries_the_same_check() {
    let profile = Profile::new();
    let mut state = profile.state_with_result(&profile.file_path, Trust::FileNotFound);
    let _lock = profile.write_lock();

    recover(
        &profile.catalog,
        &mut state,
        0,
        Action::CheckFolder,
        &message(Action::CheckFolder),
    );
    drop(_lock);

    assert_eq!(state.notice, Some(UserNotice::FolderNotChecked));
    assert!(matches!(
        retry(&state),
        Some(Message::SourceRefreshRequested(id)) if id == profile.source_id
    ));
}

#[test]
fn check_folder_for_a_file_the_catalog_does_not_know_says_so() {
    let profile = Profile::new();
    let mut state = profile.state_with_result("/nowhere/ghost.md", Trust::FileNotFound);

    recover(
        &profile.catalog,
        &mut state,
        0,
        Action::CheckFolder,
        &message(Action::CheckFolder),
    );

    assert_eq!(state.notice, Some(UserNotice::FileCouldNotBeFound));
    assert_eq!(profile.jobs("scan"), 0);
}

/// The actions orbok does elsewhere (state-only, or `result_launch`) do
/// nothing here.
#[test]
fn the_other_actions_are_not_this_modules() {
    let profile = Profile::new();
    let mut state = profile.state_with_result(&profile.file_path, Trust::NeedsUpdate);
    for action in [
        Action::RemoveFromResults,
        Action::ViewDetails,
        Action::OpenAnyway,
        Action::ShowInFolder,
    ] {
        recover(&profile.catalog, &mut state, 0, action, &message(action));
    }
    assert_eq!(profile.jobs("extract"), 0);
    assert_eq!(profile.jobs("scan"), 0);
    assert_eq!(state.notice, None);
    assert_eq!(state.search_results.len(), 1);
}
