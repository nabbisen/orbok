//! RFC-058 §6: the end-to-end reachability test. Proves the application's
//! own entry points (`bootstrap::*`, driving the real hosted scheduler --
//! not a directly-invoked worker) produce correct results against a real
//! corpus on disk, for Task 035's two assertions (RFC-058 §6 table rows 1
//! and this task's own §5.1/§5.2): a startup rescan picks up a file edited
//! while orbok was closed, and manual refresh picks up a file added while
//! orbok is running.
//!
//! Placement: `crates/app`'s own binary (`#[cfg(test)] mod` in `main.rs`),
//! not a separate `--test` integration target -- RFC-058 §11's open
//! question, resolved here by following the established `runtime_isolation_tests.rs`
//! precedent (RFC-049's own boundary tests), which avoids needing to widen
//! any `bootstrap::` function's visibility beyond `pub`/`pub(crate)` just
//! for a separate test binary to reach it.

use super::bootstrap;
use super::scheduler_host::{self, ResourceObservation};
use orbok::runtime_context::{PlatformRuntimePaths, RuntimeContext, RuntimeSelection};
use std::path::Path;
use std::time::{Duration, Instant};

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

/// Runs the real hosted scheduler (`scheduler_host::run_with_context` --
/// the same function `main.rs`'s subscription wires into
/// `Subscription::run_with`, RFC-056 §4.1) against `context`'s catalog
/// until every `index_jobs` row is out of `queued`/`running`, then aborts
/// it. This is RFC-058 §6's `drain_scheduler_until_idle`: the hosted
/// scheduler, not `run_pending` or a directly-invoked worker, so this test
/// exercises the same rehydration/dispatch path the shipped application
/// runs, including `Scan` jobs enqueued by `bootstrap::check_and_refresh_source`
/// or `bootstrap::scan_and_index_source` before this is called.
async fn drain_scheduler_until_idle(context: &RuntimeContext, timeout: Duration) {
    let loop_catalog = bootstrap::open_catalog(context).unwrap();
    let cache = bootstrap::cache_service(context).unwrap();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let (_close_tx, resource_signals) = futures::channel::mpsc::channel::<ResourceObservation>(1);
    let handle = tokio::spawn(scheduler_host::run_with_context(
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

    // A separate Catalog handle for polling -- `loop_catalog` above was
    // moved into the spawned task.
    let poll_catalog = bootstrap::open_catalog(context).unwrap();
    let start = Instant::now();
    loop {
        let queued_or_running: i64 = poll_catalog
            .lock()
            .query_row(
                "SELECT COUNT(*) FROM index_jobs WHERE status IN ('queued', 'running')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        if queued_or_running == 0 {
            break;
        }
        if start.elapsed() > timeout {
            handle.abort();
            panic!("timed out after {timeout:?} waiting for the scheduler to drain");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    handle.abort();
}

fn write_markdown(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

/// RFC-058 §6 row 1 / Task 035 §5.1: with a source registered and a file
/// edited on disk while orbok is closed, restarting orbok causes a
/// subsequent search to return the new content.
///
/// "Restarting orbok" is `bootstrap::load_initial_state` -- the real
/// startup entry point `main.rs` calls, not a scan invoked directly by
/// this test. Every function this test calls (`load_initial_state`,
/// `run_search`, `open_catalog`, `add_source`, `scan_and_index_source`)
/// already existed before Task 035 -- this assertion needed no new
/// function to become writable, and was confirmed failing against
/// `main` before `load_initial_state` was taught to enqueue a startup
/// scan.
#[tokio::test]
async fn restarting_orbok_picks_up_a_file_edited_while_closed() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    let doc = source_dir.join("doc.md");
    write_markdown(&doc, "# Doc\n\noriginalcontentmarker here.\n");

    // First launch.
    let source_id = {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) = bootstrap::add_source(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
        drop(catalog);
        drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

        let catalog = bootstrap::open_catalog(&context).unwrap();
        let results =
            bootstrap::run_search(&context, &catalog, "originalcontentmarker", 20).unwrap();
        assert!(
            !results.is_empty(),
            "baseline: the original content must be findable before any edit"
        );
        card.source_id
    };
    let _ = &source_id;

    // orbok "closes": no process, no held Catalog handle survives this
    // point. The file changes on disk with nothing running.
    write_markdown(&doc, "# Doc\n\nrevisedcontentmarker here.\n");

    // "Restart": the real startup entry point, not a scan called directly.
    let _state = bootstrap::load_initial_state(&context).unwrap();
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let results = bootstrap::run_search(&context, &catalog, "revisedcontentmarker", 20).unwrap();
    assert!(
        !results.is_empty(),
        "restarting orbok must re-scan registered sources and pick up a file \
         edited while orbok was closed"
    );
}

/// RFC-058 §6 / Task 035 §5.2: with a new file added to a registered
/// folder while orbok is running, invoking manual refresh causes a
/// subsequent search to find it.
///
/// `bootstrap::check_and_refresh_source` is the function this test calls
/// as "manual refresh" -- the same function `main.rs`'s handler for the
/// new refresh button/message calls, not a scan invoked another way.
/// Confirmed failing before this function existed at all is not
/// meaningful (there was nothing to call); confirmed failing instead by
/// mutation, the equivalent evidence Task 034 established throughout:
/// with the function's `scan_and_index_source` call temporarily removed
/// (status updates but nothing is enqueued), this test fails because the
/// new file is never found; restored, it passes. See the review request
/// for the verbatim red output.
#[tokio::test]
async fn manual_refresh_picks_up_a_file_added_while_running() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    write_markdown(
        &source_dir.join("existing.md"),
        "# Existing\n\nalreadyheremarker content.\n",
    );

    let source_id = {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) = bootstrap::add_source(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
        card.source_id
    };
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    // orbok stays "running": the source stays registered, no restart.
    // A new file appears in the folder.
    write_markdown(
        &source_dir.join("newfile.md"),
        "# New\n\nnewlyaddedmarker content.\n",
    );

    // Manual refresh: the application's own entry point for it.
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        bootstrap::check_and_refresh_source(&catalog, &source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let results = bootstrap::run_search(&context, &catalog, "newlyaddedmarker", 20).unwrap();
    assert!(
        !results.is_empty(),
        "manual refresh must re-scan the source and find a file added while \
         orbok was running"
    );
}

/// RFC-037 §8/§12, Task 035 §5.3: a file deleted from disk, once picked up
/// by a refresh, must stop appearing as a normal search result -- asserted
/// against catalog state (`files.file_status`), not the RFC-060 trust
/// badge (not yet built).
///
/// Confirmed failing against the code as `check_and_refresh_source` and
/// `Scanner::scan` stood at the start of this addition: `mark_missing_unseen`
/// flips `files.file_status` to `missing` but nothing cascaded that to the
/// file's chunks, so `chunk_status` stayed `active` and both the keyword
/// and vector search queries (which gate only on `chunk_status = 'active'`,
/// never on the owning file's status) kept returning it. Fixed by
/// `ChunkRepository::deactivate_for_missing_files`, called from
/// `Scanner::scan` right after `mark_missing_unseen` -- see that function's
/// own doc comment for why chunks are marked `stale` rather than `deleted`
/// (recoverable if the file reappears unchanged) and its `reactivate_last_stale_generation`
/// counterpart for the return path.
#[tokio::test]
async fn deleting_a_file_marks_it_missing_and_removes_it_from_search_results() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    let doc = source_dir.join("doomed.md");
    write_markdown(&doc, "# Doomed\n\nsoontobegonemarker content.\n");

    let source_id = {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) = bootstrap::add_source(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
        card.source_id
    };
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let results = bootstrap::run_search(&context, &catalog, "soontobegonemarker", 20).unwrap();
        assert!(
            !results.is_empty(),
            "baseline: the file must be findable before it is deleted"
        );
    }

    std::fs::remove_file(&doc).unwrap();

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        bootstrap::check_and_refresh_source(&catalog, &source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let results = bootstrap::run_search(&context, &catalog, "soontobegonemarker", 20).unwrap();
    assert!(
        results.is_empty(),
        "a file marked missing by refresh must stop appearing as a normal \
         search result, got {results:?}"
    );

    let (file_status, file_count): (String, i64) = catalog
        .lock()
        .query_row(
            "SELECT file_status, COUNT(*) FROM files WHERE display_path LIKE '%doomed.md'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        file_status, "missing",
        "the catalog must record the file as missing, not delete its row"
    );
    assert_eq!(
        file_count, 1,
        "refresh must not delete the file's catalog row, only mark it missing"
    );
}

/// RFC-004 §11 / Task 035 §5.3's recovery counterpart: a file that went
/// missing and then reappears with byte-identical content -- the case the
/// previous test's own doc comment names as the reason
/// `deactivate_for_missing_files` marks chunks `stale` and not `deleted` --
/// must become searchable again, through the same real refresh entry point,
/// with no new extraction (the content never changed).
#[tokio::test]
async fn restoring_a_missing_file_with_unchanged_content_makes_it_searchable_again() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    let doc = source_dir.join("comeback.md");
    let body = "# Comeback\n\ntemporarilygonemarker content.\n";
    write_markdown(&doc, body);

    let source_id = {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) = bootstrap::add_source(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
        card.source_id
    };
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    std::fs::remove_file(&doc).unwrap();
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        bootstrap::check_and_refresh_source(&catalog, &source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let results =
            bootstrap::run_search(&context, &catalog, "temporarilygonemarker", 20).unwrap();
        assert!(
            results.is_empty(),
            "sanity: must be gone from search while missing"
        );
    }

    // Same bytes, rewritten -- mtime changes, content does not (the hash
    // check path, not the fast metadata-unchanged path).
    write_markdown(&doc, body);
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        bootstrap::check_and_refresh_source(&catalog, &source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let results = bootstrap::run_search(&context, &catalog, "temporarilygonemarker", 20).unwrap();
    assert!(
        !results.is_empty(),
        "a file that reappears with unchanged content must become searchable \
         again without a new extraction"
    );
}

/// RFC-037 §12/§17.3, Task 035 §5.4: a registered folder that is gone at
/// startup (renamed or unmounted, standing in for either since
/// `check_source_path` only ever does a `stat()`) must be marked
/// `FolderNotFound` -- surfaced via `orbok_core::SourceStatus::Missing`,
/// the catalog-backed 5-state vocabulary `check_and_refresh_source`'s own
/// doc comment explains -- and nothing about it may be deleted: not the
/// source row, not its files, not their chunks (RFC-037 §12: "deletes
/// nothing").
///
/// Through the real startup entry point (`bootstrap::load_initial_state`),
/// not `check_and_refresh_source` called directly -- this is specifically
/// the *startup* half of Task 035 §4.1, proven the same way
/// `restarting_orbok_picks_up_a_file_edited_while_closed` proves the
/// startup rescan.
#[tokio::test]
async fn a_renamed_or_unmounted_folder_is_marked_missing_at_startup_and_nothing_is_deleted() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    write_markdown(
        &source_dir.join("doc.md"),
        "# Doc\n\nunmountedfoldermarker content.\n",
    );

    let source_id = {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) = bootstrap::add_source(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
        card.source_id
    };
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let results =
            bootstrap::run_search(&context, &catalog, "unmountedfoldermarker", 20).unwrap();
        assert!(
            !results.is_empty(),
            "baseline: the file must be findable before the folder disappears"
        );
    }

    // Renamed/unmounted, standing in for both: the registered canonical
    // path no longer resolves to anything.
    std::fs::remove_dir_all(&source_dir).unwrap();

    let state = bootstrap::load_initial_state(&context).unwrap();
    drain_scheduler_until_idle(&context, Duration::from_secs(5)).await;

    let card = state
        .sources
        .iter()
        .find(|c| c.source_id == source_id.as_str())
        .expect("the source must still be listed, not removed");
    assert_eq!(
        card.status,
        orbok_core::SourceStatus::Missing,
        "a folder gone at startup must be marked Missing (RFC-037 FolderNotFound), \
         not silently dropped or errored"
    );

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let (source_count, file_count): (i64, i64) = catalog
        .lock()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM sources WHERE source_id = ?1), \
                    (SELECT COUNT(*) FROM files WHERE source_id = ?1)",
            [source_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        source_count, 1,
        "RFC-037 §12 'deletes nothing': the source row must survive"
    );
    assert_eq!(
        file_count, 1,
        "RFC-037 §12 'deletes nothing': the file row must survive"
    );
}
