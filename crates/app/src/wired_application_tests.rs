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
//!
//! RFC-058 §6 rows 3 and 4 (kind filter, folder scope) are **not** in this
//! file, and that is a stop condition (handoff §6), not an omission.
//! `bootstrap::run_search`/`run_search_with` take `(context, catalog,
//! query, limit)` -- no filter, no scope parameter exists anywhere on the
//! call path down to `HybridSearchService::search`. There is no lower-level
//! entry point either (unlike row 5's source-pausing, which reuses
//! `SourceRepository::set_status`, a real persistence path the application
//! already calls elsewhere): `ActiveFilter`/`SearchFolderScope` live only
//! in UI state and never reach a query anywhere in the stack. Writing
//! either assertion would mean adding a parameter to the search entry
//! point myself -- exactly what RFC-060 §7 says is its own job ("grows a
//! request struct") -- which is the specific thing this handoff's scope
//! rule warns against. Reported per the handoff's own stop condition
//! rather than force-written.

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

/// A real, valid three-page PDF, built with `lopdf`'s own document/writer
/// API rather than hand-authored byte content (`Document::save` computes
/// its own xref table, which a hand-edited multi-page extension of a
/// known-good fixture would risk corrupting) -- each page carries its own
/// distinct marker text, so a query can target one specific page.
///
/// Page object IDs are reserved **first**, before any other object, so
/// they come out numbered 1, 2, 3 in page order. **Historical note, no
/// longer load-bearing as of RFC-060 Amendment 1 / HANDOFF-060 slice 1**:
/// this ordering was originally a workaround for a real, separate defect
/// found while building this fixture -- `crates/pipeline/extract/src/pdf.rs`'s
/// extraction loop called `lopdf::Document::extract_text(&[*obj_id])` with
/// the page's *object* ID where `extract_text` wants a 1-based *page
/// number*, so a page whose object ID didn't equal its page number (true
/// of essentially every real-world PDF) extracted no text at all. That is
/// now fixed (`pdf.rs` uses `page_num`; see
/// `orbok-extract::tests::pdf_extraction_finds_every_page_regardless_of_object_numbering`,
/// which deliberately constructs the *opposite* object numbering to prove
/// it). This function's page-IDs-first ordering is kept as-is since it is
/// still a valid, working fixture and there is no reason to touch it
/// further, not because the ordering still matters for extraction to
/// succeed.
fn write_three_page_pdf(path: &Path, page_texts: [&str; 3]) {
    use lopdf::content::{Content, Operation};
    use lopdf::{Document, Object, Stream, dictionary};

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }

    let mut doc = Document::with_version("1.5");
    let page_ids: Vec<(u32, u16)> = (0..page_texts.len()).map(|_| doc.new_object_id()).collect();
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let mut page_refs: Vec<Object> = Vec::new();
    for (page_id, text) in page_ids.iter().zip(page_texts) {
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 12.into()]),
                Operation::new("Td", vec![72.into(), 700.into()]),
                Operation::new("Tj", vec![Object::string_literal(text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        doc.objects.insert(
            *page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
            }),
        );
        page_refs.push((*page_id).into());
    }

    let pages = dictionary! {
        "Type" => "Pages",
        "Count" => page_refs.len() as i64,
        "Kids" => page_refs,
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.save(path).unwrap();
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

/// RFC-058 §6 row 6 / RFC-060 §11.1 (F-03): a PDF result's snippet must
/// contain real text from the matched page, not the raw bytes that result
/// from treating a stored page number as a text-file line number.
///
/// **Updated for RFC-060 Amendment 1 / HANDOFF-060 slice 1.** This test
/// used to fail with raw `%PDF-1.5` / `1 0 obj` syntax in the snippet,
/// because the whole-file "document" chunk (`chunker.rs`'s aggregate,
/// RFC-060 §10 -- matches nearly any query, so it is typically the rank-1
/// result) hardcoded `location_quality: "exact"` regardless of the
/// segments it spanned, so Task 034's interim guard (`load_snippet`
/// returns `None` unless `location_quality == "exact"`) never applied to
/// it. Slice 1 fixed that (`chunker.rs` now derives quality from spanned
/// segments, `crates/pipeline/extract/src/types.rs::chunk_location_quality`)
/// and separately fixed the extraction bug that made every PDF page
/// unreadable in the first place (`pdf.rs:116` was passing an object ID
/// where `extract_text` wants a page number). With both fixed, the guard
/// now correctly fires for the document chunk, so the snippet is `None`
/// (empty) rather than garbage -- **this is the expected next failure
/// state, not a regression** (handoff §3.3 predicted exactly this and
/// named it "correct behaviour, not a regression"). RFC-060 §5/§6
/// (persisting `location_kind`, rendering non-`Lines` snippets from the
/// extraction cache) is what makes a real snippet appear; until then this
/// stays `#[should_panic]`, not deleted and not un-panicked, because the
/// criterion itself -- a PDF result's snippet contains real page text --
/// is still unmet.
#[tokio::test]
#[should_panic(expected = "snippet must contain real text from page 3")]
async fn pdf_result_snippet_contains_page_text_not_raw_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    write_three_page_pdf(
        &source_dir.join("doc.pdf"),
        [
            "firstpagemarker content on page one.",
            "secondpagemarker content on page two.",
            "thirdpagemarker content on page three.",
        ],
    );

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) = bootstrap::add_source(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let results = bootstrap::run_search(&context, &catalog, "thirdpagemarker", 20).unwrap();
    assert!(
        !results.is_empty(),
        "the PDF must be findable by text unique to its third page"
    );
    let snippet = results[0].snippet.as_deref().unwrap_or("");
    assert!(
        !snippet.contains(" obj") && !snippet.contains("endobj"),
        "snippet must not contain raw PDF object syntax, got {snippet:?}"
    );
    assert!(
        snippet.contains("thirdpagemarker"),
        "snippet must contain real text from page 3, got {snippet:?}"
    );
}

/// RFC-058 §6 row 2 / RFC-060 §11.3 (F-06): a result's trust state must
/// reflect the file's real state, and today it never does --
/// `bootstrap/search.rs` hardcodes `ResultTrustDisplay::default()` (state
/// `Ready`, no recovery actions) on every result, regardless of the file
/// backing it.
///
/// Deliberately does **not** call `check_and_refresh_source` after
/// deleting the file. A refresh would mark the file `missing` and
/// deactivate its chunks (Task 035), which excludes it from search results
/// entirely -- correct for that scenario, but it means the file could never
/// appear in a result to carry a trust badge on. The gap F-06 describes is
/// the window this test occupies instead: a file gone from disk that
/// **no refresh has processed yet**, so the catalog still calls it
/// `indexed`, search still returns it, and the trust the result carries is
/// wrong regardless -- always `Ready`, never reflecting that the file
/// backing it no longer exists.
///
/// `#[should_panic]`, not `#[ignore]` (RFC-058 §6 Group B): remove the
/// wrapper once RFC-060 §7 wires `SearchResultTrust::from_catalog` into
/// this path.
#[tokio::test]
#[should_panic(expected = "must not be Ready for a file deleted from disk")]
async fn a_result_for_a_file_deleted_from_disk_is_not_labelled_ready() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    let doc = source_dir.join("vanishing.md");
    write_markdown(&doc, "# Vanishing\n\nvanishingfilemarker content.\n");

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) = bootstrap::add_source(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let results = bootstrap::run_search(&context, &catalog, "vanishingfilemarker", 20).unwrap();
        assert!(
            !results.is_empty(),
            "baseline: the file must be findable before it is deleted"
        );
    }

    // Deleted, but no refresh has run: the catalog does not know yet.
    std::fs::remove_file(&doc).unwrap();

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let results = bootstrap::run_search(&context, &catalog, "vanishingfilemarker", 20).unwrap();
    assert!(
        !results.is_empty(),
        "sanity: with no refresh run, the stale catalog entry must still surface a result \
         -- otherwise this is no longer the window F-06 describes"
    );
    assert_ne!(
        results[0].trust.state,
        orbok_ui::state::ResultTrustDisplay::default().state,
        "a result's trust state must not be Ready for a file deleted from disk, \
         got {:?}",
        results[0].trust
    );
}

/// RFC-058 §6 row 5 / RFC-060 §11.6 (F-07, source-level): a source set to
/// `Paused` must contribute no results, and today it still does -- no
/// retrieval query joins `sources` (RFC-060 §7's own table). Task 035
/// closed only the *file*-level half of row 5 (a missing file's chunks are
/// deactivated); this is the untouched *source*-level half, per the
/// handoff's own note that no assumption should be made that Task 035
/// covered it.
///
/// Paused via `SourceRepository::set_status` directly -- the same
/// repository method `bootstrap::check_and_refresh_source` itself calls to
/// persist a status change -- because no UI action pauses a source yet
/// (`bootstrap/startup.rs`'s own comment on `SourceStatus::Paused` says so).
/// This sets up the precondition through the application's real persistence
/// path, the same way other tests in this file use `std::fs::remove_file`
/// to arrange a precondition, and then observes the outcome through the
/// real entry point under test, `bootstrap::run_search`.
///
/// `#[should_panic]`, not `#[ignore]` (RFC-058 §6 Group B): this runs on
/// every push and fails loudly, naming the RFC-060 criterion, if the guard
/// disappears without anyone removing this wrapper. Remove the wrapper once
/// RFC-060 §7 makes source status honoured at the query layer.
#[tokio::test]
#[should_panic(expected = "a paused source's files must not appear in search results")]
async fn a_paused_source_contributes_no_search_results() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    write_markdown(
        &source_dir.join("doc.md"),
        "# Doc\n\npausedsourcemarker content.\n",
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
        let results = bootstrap::run_search(&context, &catalog, "pausedsourcemarker", 20).unwrap();
        assert!(
            !results.is_empty(),
            "baseline: the file must be findable before its source is paused"
        );
    }

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        orbok_db::repo::SourceRepository::new(&catalog)
            .set_status(
                &orbok_core::SourceId::from_string(source_id.clone()),
                orbok_core::SourceStatus::Paused,
            )
            .unwrap();
    }

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let results = bootstrap::run_search(&context, &catalog, "pausedsourcemarker", 20).unwrap();
    assert!(
        results.is_empty(),
        "a paused source's files must not appear in search results, got {results:?}"
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

/// RFC-058 §6 row 8 / RFC-060 §11.8 (F-02, F-02b): a short, dense Japanese
/// chunk containing the query term must outrank a long chunk that mentions
/// it once, buried in filler -- through the real application entry point,
/// not `MultilingualKeywordEngine` invoked directly (that already has this
/// exact corpus at the library level,
/// `orbok-search`'s `task034_ranking_fusion::cjk_merge_ranks_the_dense_relevant_chunk_first`,
/// which is proof the library is correct, not that the app calls it
/// correctly -- RFC-058's own point). Same term ("認証エラー") and filler
/// text as that test, verified there against real FTS5 `bm25()` scores
/// (short: -1.3253e-6, long: -8.0292e-7, lower is better) -- reused rather
/// than re-derived.
///
/// Needs no embedding model: `contains_cjk` routes this query through
/// `MultilingualKeywordEngine`'s unicode61+trigram merge
/// (`rrf_fuse_keyword_lists`) regardless of search mode, so this exercises
/// the fix Task 034 landed (`multilingual.rs`'s comparator direction) via
/// keyword-only search alone.
#[tokio::test]
async fn japanese_query_ranks_the_dense_relevant_chunk_first() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");

    write_markdown(&source_dir.join("short.md"), "# Short\n\n認証エラー\n");
    let filler = "今日は天気がとても良いので散歩に出かけました。".repeat(2);
    write_markdown(
        &source_dir.join("long.md"),
        &format!("# Long\n\n{filler}認証エラー。{filler}\n"),
    );

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) = bootstrap::add_source(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let results = bootstrap::run_search(&context, &catalog, "認証エラー", 20).unwrap();
    assert!(!results.is_empty(), "the corpus must be findable at all");
    // RFC-060 §10's own recorded, separate defect: the whole-file "document"
    // chunk isn't deduped from the section-level chunk, so each file can
    // appear twice. Not this test's concern -- dedupe by path to check
    // ordering between the two *files*, which is what row 8 is about.
    let mut first_rank_by_file: Vec<&str> = Vec::new();
    for r in &results {
        if !first_rank_by_file.contains(&r.display_path.as_str()) {
            first_rank_by_file.push(&r.display_path);
        }
    }
    assert_eq!(
        first_rank_by_file.len(),
        2,
        "both files must be found: {results:?}"
    );
    assert!(
        first_rank_by_file[0].ends_with("short.md"),
        "the short, dense chunk must rank first -- got file order {first_rank_by_file:?}"
    );
}

/// RFC-058 §6 row 7 / RFC-060 §11.7 (F-10): two identical searches issued
/// in one process must return identical result orders, over 20
/// repetitions. The fix (`rrf_fuse`'s `chunk_id` tie-break, Task 034 §2)
/// only has anything to prove itself against when two candidates'
/// `rrf_score`s **tie** -- `1/(60+kw_rank) + 1/(60+vec_rank)` landing on
/// the same value for two different chunks, e.g. one at keyword-rank 1 /
/// vector-rank 5 and another at keyword-rank 5 / vector-rank 1 (the
/// structural case `rrf.rs`'s own comment names). A single keyword list
/// can never produce this -- ranks 1..N are already unique, so
/// `rrf_fuse`'s score is too, with nothing to tie -- fusing keyword
/// candidates with an **empty** vector list (`SearchCapability::KeywordOnly`,
/// this repo's normal state) cannot exercise the defect no matter how the
/// corpus is shaped. Only a real hybrid search, with real vector
/// candidates from a real embedding model, produces the tie.
///
/// `#[ignore]`d for the same reason `bootstrap::tests::embedding_blocking_measurement`
/// is (no ONNX model file is available in CI or this sandbox), gated the
/// way RFC-058 §11 open question 2 asks model-dependent assertions to be:
/// not silently skipped, run manually. **Not executed as part of this
/// task** -- I could not construct or verify a genuine tie without a real
/// model to check ranks against, so I cannot report having observed this
/// one green, let alone red-then-green under mutation. Whoever next has
/// `RFC013_MODEL_DIR` available should run it once (with `-p orbok --bin
/// orbok --features orbok-embed/tract --release -- --ignored`), confirm
/// it passes, then mutate `rrf_fuse`'s `chunk_id` tie-break away (delete
/// the `.then_with(...)` in `crates/search/engine/src/rrf.rs`) and confirm
/// it goes red, the same way row 8's mutation was carried out and recorded
/// here.
///
/// ```sh
/// RFC013_MODEL_DIR=~/.local/share/orbok/models/multilingual-e5-small \
///   cargo test -p orbok --bin orbok --features orbok-embed/tract --release \
///   two_identical_searches_return_identical_orders -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "requires the real embedding model on disk; see RFC013_MODEL_DIR"]
async fn two_identical_searches_return_identical_orders() {
    let model_dir = std::env::var("RFC013_MODEL_DIR").expect(
        "RFC013_MODEL_DIR must point at the multilingual-e5-small model directory \
         (e.g. ~/.local/share/orbok/models/multilingual-e5-small)",
    );

    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    crate::settings::save_settings(
        &temp.path().join("settings.json"),
        &crate::settings::OrbokSettings {
            embedding_model_dir: Some(model_dir),
            ..Default::default()
        },
    )
    .unwrap();

    let source_dir = temp.path().join("source");
    // A modest field of candidates sharing the query term but otherwise
    // varied, so keyword rank and semantic (vector) rank are unlikely to
    // agree file-for-file -- the condition under which `rrf_fuse` produces
    // a structural tie somewhere in the set. Which pair ties, if any,
    // cannot be predicted without the real model; the assertion below
    // does not depend on knowing which -- only that whichever order comes
    // back is the same every time.
    for i in 0..8 {
        write_markdown(
            &source_dir.join(format!("doc{i}.md")),
            &format!(
                "# Document {i}\n\n\
                 Notes on authentication token rotation and related topics, \
                 variant {i}, covering configuration step {i} in some detail.\n"
            ),
        );
    }

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) = bootstrap::add_source(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(60)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let first =
        bootstrap::run_search(&context, &catalog, "authentication token rotation", 20).unwrap();
    assert!(!first.is_empty(), "the corpus must be findable at all");
    let first_order: Vec<String> = first.iter().map(|r| r.display_path.clone()).collect();

    for i in 1..20 {
        let repeat =
            bootstrap::run_search(&context, &catalog, "authentication token rotation", 20).unwrap();
        let order: Vec<String> = repeat.iter().map(|r| r.display_path.clone()).collect();
        assert_eq!(
            order, first_order,
            "search {i} returned a different order than search 0 -- \
             identical searches must return identical result orders"
        );
    }
}
