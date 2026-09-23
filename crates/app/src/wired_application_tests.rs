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
//! `bootstrap::run_search` takes `(catalog, model, query, limit)` -- no
//! filter, no scope parameter exists anywhere on the
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
use orbok::runtime_context::{
    AllowRuntimePathProbe, PlatformRuntimePaths, RuntimeContext, RuntimeSelection,
};
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
    drain_scheduler_until_idle_with_embedding(context, timeout, None).await;
}

/// `drain_scheduler_until_idle`, but with a caller-supplied embedding
/// model wired into the hosted scheduler instead of always running with
/// `embedding_parts: None`. Every other caller stays keyword-only (no
/// reason to pay for a model load in tests that never search by vector);
/// `two_identical_searches_return_identical_orders` (RFC013_MODEL_DIR)
/// is the one exception -- it needs `GenerateEmbedding` jobs to actually
/// produce vectors, not fall back to `model_missing` (RFC-008 §15).
async fn drain_scheduler_until_idle_with_embedding(
    context: &RuntimeContext,
    timeout: Duration,
    embedding_parts: Option<crate::bootstrap::embedding_resolution::EmbeddingWorkerParts>,
) {
    let loop_catalog = bootstrap::open_catalog(context).unwrap();
    let cache = bootstrap::cache_service(context).unwrap();
    let (tx, rx) = futures::channel::mpsc::channel(64);
    let (_close_tx, resource_signals) = futures::channel::mpsc::channel::<ResourceObservation>(1);
    let handle = tokio::spawn(scheduler_host::run_with_context(
        loop_catalog,
        cache,
        crate::scheduler_host::EmbeddingSource::fixed(embedding_parts),
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
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
        drop(catalog);
        drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

        let catalog = bootstrap::open_catalog(&context).unwrap();
        let results = bootstrap::run_search(
            &catalog,
            None,
            None,
            "originalcontentmarker",
            orbok_search::SearchMode::Auto,
            20,
            orbok_core::SearchScope::default(),
        )
        .unwrap();
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
    let results = bootstrap::run_search(
        &catalog,
        None,
        None,
        "revisedcontentmarker",
        orbok_search::SearchMode::Auto,
        20,
        orbok_core::SearchScope::default(),
    )
    .unwrap();
    assert!(
        !results.is_empty(),
        "restarting orbok must re-scan registered sources and pick up a file \
         edited while orbok was closed"
    );
}

/// Task 079 §1.3: a namespace this project has retired is purged the next
/// time orbok starts, through the real startup entry point.
#[test]
fn a_retired_cache_namespace_is_purged_on_the_next_start() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let db_path = temp.path().join(orbok_db::CACHE_FILE_NAME);

    let stale_file = temp.path().join("stale.md");
    write_markdown(&stale_file, "# Stale\n\nstale.\n");
    let retired = localcache::CacheEngine::<serde_json::Value>::builder()
        .database(&db_path)
        .namespace("extract-segments:v1".to_string())
        .change_detection(localcache::ChangeDetectionMode::MetadataThenFullHash)
        .build()
        .unwrap();
    retired
        .set(
            std::fs::canonicalize(&stale_file).unwrap(),
            &serde_json::json!({"stale": true}),
        )
        .unwrap();
    assert!(
        !retired.keys(None).unwrap().is_empty(),
        "the retired-namespace entry must exist before startup, or this proves nothing"
    );
    drop(retired);

    let _state = bootstrap::load_initial_state(&context).unwrap();

    let retired_after = localcache::CacheEngine::<serde_json::Value>::builder()
        .database(&db_path)
        .namespace("extract-segments:v1".to_string())
        .change_detection(localcache::ChangeDetectionMode::MetadataThenFullHash)
        .build()
        .unwrap();
    assert!(
        retired_after.keys(None).unwrap().is_empty(),
        "starting orbok must purge the retired extract-segments:v1 namespace"
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
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
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
    let results = bootstrap::run_search(
        &catalog,
        None,
        None,
        "newlyaddedmarker",
        orbok_search::SearchMode::Auto,
        20,
        orbok_core::SearchScope::default(),
    )
    .unwrap();
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
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
        card.source_id
    };
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let results = bootstrap::run_search(
            &catalog,
            None,
            None,
            "soontobegonemarker",
            orbok_search::SearchMode::Auto,
            20,
            orbok_core::SearchScope::default(),
        )
        .unwrap();
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
    let results = bootstrap::run_search(
        &catalog,
        None,
        None,
        "soontobegonemarker",
        orbok_search::SearchMode::Auto,
        20,
        orbok_core::SearchScope::default(),
    )
    .unwrap();
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
/// **The wrapper is removed by RFC-060 Slice 3**, which is what §5/§6 said
/// would remove it: migration 0008 persists `location_kind`, so the snippet
/// path knows a PDF chunk's positions are page numbers, and renders its
/// snippet from the cached extraction segments instead of reading "those
/// lines" out of the file. The criterion -- a PDF result's snippet contains
/// real page text, and no object syntax -- is met.
#[tokio::test]
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
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    // RFC-060 §6: a PDF's positions are page numbers, so its snippet comes
    // from the cached extraction segments -- the search path needs the same
    // cache handle `main.rs` gives it in production.
    let cache = bootstrap::cache_service(&context).unwrap();
    let results = bootstrap::run_search(
        &catalog,
        None,
        Some(cache.service()),
        "thirdpagemarker",
        orbok_search::SearchMode::Auto,
        20,
        orbok_core::SearchScope::default(),
    )
    .unwrap();
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

/// RFC-060 §11 criterion 2: a DOCX and an HTML result's snippets contain
/// document text, or are empty with the result still shown -- never raw
/// markup or binary.
///
/// Both formats store *approximate* positions (paragraph and block
/// indices, `v09_rc.rs` asserts this for the extractors themselves), so
/// before Slice 3 the snippet path either read those numbers as file line
/// numbers -- returning ZIP bytes for a DOCX and tag soup for an HTML
/// file -- or, after Task 034's interim quality guard, returned nothing at
/// all. Now `location_kind` says what the numbers mean and the text comes
/// from the cached extraction segments.
///
/// The assertion is deliberately the criterion's own disjunction: text or
/// empty. It is not "a snippet exists", because RFC-060 Amendment 3 rules
/// that an absent cache entry means an absent snippet, and that outcome
/// must stay passing rather than turn this red.
#[tokio::test]
async fn docx_and_html_snippets_contain_document_text_never_markup() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    // Multi-line on purpose: a single-line fixture cannot distinguish the
    // fix from the defect, because reading "block 2" as line 2 of a
    // one-line file skips past the end and yields an empty snippet, which
    // this criterion allows. Real HTML has lines, and reading one returns
    // markup.
    std::fs::write(
        source_dir.join("page.html"),
        "<html>\n<head><title>t</title></head>\n<body>\n<h1>Guide</h1>\n\
         <p>htmlmarkerword appears in a paragraph.</p>\n</body>\n</html>\n",
    )
    .unwrap();
    std::fs::write(
        source_dir.join("doc.docx"),
        minimal_docx("docxmarkerword appears in a paragraph."),
    )
    .unwrap();

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let cache = bootstrap::cache_service(&context).unwrap();
    for (query, marker) in [
        ("htmlmarkerword", "htmlmarkerword"),
        ("docxmarkerword", "docxmarkerword"),
    ] {
        let results = bootstrap::run_search(
            &catalog,
            None,
            Some(cache.service()),
            query,
            orbok_search::SearchMode::Auto,
            20,
            orbok_core::SearchScope::default(),
        )
        .unwrap();
        assert!(
            !results.is_empty(),
            "{query}: the file must be findable by text unique to it"
        );
        let snippet = results[0].snippet.as_deref().unwrap_or("");
        for forbidden in ["<p>", "<html", "</", "PK\u{3}\u{4}", "word/document.xml"] {
            assert!(
                !snippet.contains(forbidden),
                "{query}: snippet must never contain raw markup or binary, \
                 found {forbidden:?} in {snippet:?}"
            );
        }
        assert!(
            snippet.is_empty() || snippet.contains(marker),
            "{query}: snippet must contain document text or be empty, got {snippet:?}"
        );
    }
}

/// RFC-060 §11 criterion 4 (RFC-058 §6 row 3, now writable): a kind
/// filter actually filters, at the query.
///
/// **Vocabulary note.** The criterion says "the Documents filter returns
/// the `.pdf`". This codebase's `KindFilter` has a dedicated `Pdfs` kind,
/// and `Documents` means Office documents (`docx`, `doc`, `odt`, `rtf`) --
/// so the filter that selects a PDF here is `Pdfs`. The assertion is the
/// criterion's: with the filter, the `.pdf` and not the `.md`; without it,
/// both.
#[tokio::test]
async fn a_kind_filter_returns_only_that_kind() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    write_three_page_pdf(
        &source_dir.join("doc.pdf"),
        [
            "kindfiltermarker on page one.",
            "second page.",
            "third page.",
        ],
    );
    std::fs::write(
        source_dir.join("note.md"),
        "# Note\n\nkindfiltermarker in a markdown note.\n",
    )
    .unwrap();

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let cache = bootstrap::cache_service(&context).unwrap();
    let paths_for = |scope: orbok_core::SearchScope| -> Vec<String> {
        bootstrap::run_search(
            &catalog,
            None,
            Some(cache.service()),
            "kindfiltermarker",
            orbok_search::SearchMode::Auto,
            20,
            scope,
        )
        .unwrap()
        .into_iter()
        .map(|r| r.display_path)
        .collect()
    };

    let unfiltered = paths_for(orbok_core::SearchScope::default());
    assert!(
        unfiltered.iter().any(|p| p.ends_with(".pdf"))
            && unfiltered.iter().any(|p| p.ends_with(".md")),
        "without a filter both files must match, got {unfiltered:?}"
    );

    // Through the same conversion the UI uses, not a hand-built scope.
    let pdfs_only = bootstrap::scope_from_ui(
        &[orbok_search::ActiveFilter::Kind {
            value: orbok_search::KindFilter::Pdfs,
            label: "PDFs".to_string(),
        }],
        None,
    );
    let filtered = paths_for(pdfs_only);
    assert!(
        !filtered.is_empty() && filtered.iter().all(|p| p.ends_with(".pdf")),
        "the PDFs filter must return the .pdf and not the .md, got {filtered:?}"
    );
}

/// RFC-060 §11 criterion 5 (RFC-058 §6 row 4, now writable): a search
/// scoped to one folder returns only that folder's file.
#[tokio::test]
async fn a_search_scoped_to_one_folder_excludes_the_other() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let folder_a = temp.path().join("a");
    let folder_b = temp.path().join("b");
    std::fs::create_dir_all(&folder_a).unwrap();
    std::fs::create_dir_all(&folder_b).unwrap();
    std::fs::write(folder_a.join("a.md"), "folderscopemarker in folder a\n").unwrap();
    std::fs::write(folder_b.join("b.md"), "folderscopemarker in folder b\n").unwrap();

    let source_a = {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (a, _) =
            bootstrap::add_source_expect_added(&catalog, &folder_a.to_string_lossy()).unwrap();
        let (b, _) =
            bootstrap::add_source_expect_added(&catalog, &folder_b.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &a.source_id).unwrap();
        bootstrap::scan_and_index_source(&catalog, &b.source_id).unwrap();
        a.source_id
    };
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let cache = bootstrap::cache_service(&context).unwrap();
    let paths_for = |scope: orbok_core::SearchScope| -> Vec<String> {
        bootstrap::run_search(
            &catalog,
            None,
            Some(cache.service()),
            "folderscopemarker",
            orbok_search::SearchMode::Auto,
            20,
            scope,
        )
        .unwrap()
        .into_iter()
        .map(|r| r.display_path)
        .collect()
    };

    let both = paths_for(orbok_core::SearchScope::default());
    assert!(
        both.iter().any(|p| p.ends_with("a.md")) && both.iter().any(|p| p.ends_with("b.md")),
        "unscoped, a file in each folder must match, got {both:?}"
    );

    let scoped = paths_for(bootstrap::scope_from_ui(
        &[],
        Some(&orbok_ui::state::SearchLocation::remembered(
            orbok_core::SourceId::from_string(source_a.clone()),
            "a",
        )),
    ));
    assert!(
        !scoped.is_empty() && scoped.iter().all(|p| p.ends_with("a.md")),
        "scoped to folder a, only a's file may come back, got {scoped:?}"
    );
}

/// A minimal valid DOCX: a ZIP carrying one `word/document.xml` with two
/// paragraphs. Mirrors `orbok-workers`' own `minimal_docx` fixture rather
/// than checking a binary file into the tree.
fn minimal_docx(first_paragraph: &str) -> Vec<u8> {
    use std::io::Write;
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>
<w:p><w:r><w:t>{first_paragraph}</w:t></w:r></w:p>
<w:p><w:r><w:t>Second paragraph here.</w:t></w:r></w:p>
</w:body></w:document>"#
    );
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", opts).unwrap();
        zip.write_all(b"<Types/>").unwrap();
        zip.start_file("word/document.xml", opts).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
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
/// **The wrapper is removed by RFC-060 Slice 4**, which wires
/// `SearchResultTrust::from_catalog` into this path. The catalog's own
/// `file_status` alone would not close this window -- it still reads
/// `indexed` until a refresh runs -- so enrichment reports a file that is
/// no longer on disk as not found regardless of the row.
#[tokio::test]
async fn a_result_for_a_file_deleted_from_disk_is_not_labelled_ready() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    let doc = source_dir.join("vanishing.md");
    write_markdown(&doc, "# Vanishing\n\nvanishingfilemarker content.\n");

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let results = bootstrap::run_search(
            &catalog,
            None,
            None,
            "vanishingfilemarker",
            orbok_search::SearchMode::Auto,
            20,
            orbok_core::SearchScope::default(),
        )
        .unwrap();
        assert!(
            !results.is_empty(),
            "baseline: the file must be findable before it is deleted"
        );
    }

    // Deleted, but no refresh has run: the catalog does not know yet.
    std::fs::remove_file(&doc).unwrap();

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let results = bootstrap::run_search(
        &catalog,
        None,
        None,
        "vanishingfilemarker",
        orbok_search::SearchMode::Auto,
        20,
        orbok_core::SearchScope::default(),
    )
    .unwrap();
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

// ── HANDOFF-038: the trust state, end to end ─────────────────────────────

/// A search through the path `main.rs` uses: with the extraction cache
/// handle, which is where a result's warnings (and so its trust) come from.
fn search_marker(
    context: &RuntimeContext,
    catalog: &orbok_db::Catalog,
    marker: &str,
) -> Vec<orbok_ui::state::SearchResultDisplay> {
    let cache = bootstrap::cache_service(context).unwrap();
    bootstrap::run_search(
        catalog,
        None,
        Some(cache.service()),
        marker,
        orbok_search::SearchMode::Auto,
        20,
        orbok_core::SearchScope::default(),
    )
    .unwrap()
}

/// A markdown file with more paragraphs than the extractor's segment limit,
/// so a real extraction of it ends with `SizeLimitReached` (RFC-044 §9.5).
fn write_oversize_markdown(path: &Path) {
    let mut body = String::from("# Big\n\nrealwarningmarker is early in the file.\n\n");
    for i in 0..20_010 {
        body.push_str(&format!("paragraph number {i}\n\n"));
    }
    write_markdown(path, &body);
}

/// RFC-038 §16 criteria 3 and 6 (HANDOFF-038 Slice 1): an extraction warning
/// is represented in the result's trust, and a partly prepared file is honest
/// about it **and still searchable**.
///
/// A real file, through the real hosted scheduler: its extraction ends with
/// `SizeLimitReached`, the warning goes into the extraction cache and comes
/// back out, and the search returns the file as `PartlyPrepared`, carrying
/// that warning, with `ViewDetails` on offer.
///
/// This was a `should_panic` "known defect" test until Task 077: a file whose
/// extraction emitted *any* warning never became searchable, because
/// `ExtractWarning` was internally tagged and the cache's codec could not read
/// it back. It is now the end-to-end evidence for those two criteria.
#[tokio::test]
async fn a_real_file_with_an_extraction_warning_is_partly_prepared_and_still_found() {
    use orbok_search::{ResultRecoveryAction, ResultTrustState, ResultWarningSummary};
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    write_oversize_markdown(&source_dir.join("big.md"));
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(60)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let results = search_marker(&context, &catalog, "realwarningmarker");
    assert_eq!(
        results.len(),
        1,
        "a partly prepared file is still searchable"
    );
    assert_eq!(results[0].trust.state, ResultTrustState::PartlyPrepared);
    assert_eq!(
        results[0].trust.warnings,
        vec![ResultWarningSummary::SizeLimitReached]
    );
    assert!(
        results[0]
            .trust
            .recovery_actions
            .contains(&ResultRecoveryAction::ViewDetails),
        "the detail is one press away, got {:?}",
        results[0].trust.recovery_actions
    );
}

/// Every `ExtractWarning` variant an extraction can produce.
///
/// The exhaustive `match` is the guard: a new variant fails to compile here
/// until it is added to the list below it, so a new variant cannot ship
/// without a round trip through the cache. (Task 077: an internally tagged
/// variant set could be written to the cache and never read back.)
fn every_extract_warning() -> Vec<orbok_extract::ExtractWarning> {
    use orbok_extract::ExtractWarning as W;
    let all = vec![
        W::SomeContentSkipped {
            reason: "a reason".into(),
        },
        W::SomePagesUnreadable { pages: vec![2, 5] },
        W::PossiblyScannedPdf,
        W::SizeLimitReached {
            limit_name: "segments".into(),
        },
        W::EncodingUnsupported,
        W::UnsupportedDocumentPart {
            part: "footnotes".into(),
        },
        W::ApproximateLocationOnly,
        W::MalformedContentRecovered,
    ];
    for w in &all {
        match w {
            W::SomeContentSkipped { .. }
            | W::SomePagesUnreadable { .. }
            | W::PossiblyScannedPdf
            | W::SizeLimitReached { .. }
            | W::EncodingUnsupported
            | W::UnsupportedDocumentPart { .. }
            | W::ApproximateLocationOnly
            | W::MalformedContentRecovered => {}
        }
    }
    all
}

/// The extraction cache reads back what it was given, for every warning an
/// extraction can carry (Task 077): each variant alone, and all of them
/// together, through the real cache engine and its real codec.
#[tokio::test]
async fn an_extraction_with_a_warning_can_be_read_back_from_the_cache() {
    use orbok_cache::{CacheService, OrbokCacheNamespace};
    use orbok_extract::ExtractOutput;
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    let doc = source_dir.join("small.md");
    write_markdown(&doc, "# Small\n\ncontent.\n");
    let catalog = bootstrap::open_catalog(&context).unwrap();
    bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
    let cache = bootstrap::cache_service(&context).unwrap();
    let engine = cache
        .engine::<ExtractOutput>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            OrbokCacheNamespace::ExtractSegments.default_engine_options(),
        )
        .unwrap();
    let validated = orbok_search::snippet::searchable_path_guard(&catalog)
        .unwrap()
        .validate(&doc)
        .unwrap();

    let every = every_extract_warning();
    let mut cases: Vec<Vec<orbok_extract::ExtractWarning>> =
        every.iter().cloned().map(|w| vec![w]).collect();
    cases.push(every);
    for warnings in cases {
        let output = ExtractOutput {
            extractor_name: "test".into(),
            extractor_version: "1".into(),
            normalization_version: "1".into(),
            segments: vec![],
            char_count: 0,
            warnings: warnings.clone(),
        };
        CacheService::put(&engine, &validated, &output).unwrap();
        let back = CacheService::get_fresh(&engine, &validated)
            .unwrap_or_else(|e| panic!("reading back {warnings:?} failed: {e}"));
        assert_eq!(back, Some(output), "round trip of {warnings:?}");
    }
}

/// Task 077: an entry written under the namespace this fix retired is never
/// read. A profile upgraded mid-index has such entries, written in the old
/// shape (an internally tagged warning, which the new reader would misread).
///
/// The file is `discovered` with a chunk job queued and an old-shape entry
/// under `extract-segments:v1`, which is where an upgraded profile's
/// in-flight file would be. The chunk job must see a miss -- not decode the
/// old entry -- and fail as `extraction_cache_missing`, queuing a fresh
/// extraction (Task 056); the file then ends searchable and `PartlyPrepared`.
#[tokio::test]
async fn an_entry_from_before_the_namespace_bump_is_not_read_and_the_file_is_re_extracted() {
    use orbok_core::JobType;
    use orbok_db::repo::{FileRepository, IndexJobRepository};
    use orbok_search::ResultTrustState;

    /// The shape `ExtractWarning` had before Task 077, internally tagged.
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "snake_case", tag = "kind")]
    enum OldWarning {
        SizeLimitReached { limit_name: String },
    }
    #[derive(serde::Serialize, serde::Deserialize)]
    struct OldOutput {
        extractor_name: String,
        extractor_version: String,
        normalization_version: String,
        segments: Vec<orbok_extract::ExtractedSegment>,
        char_count: u64,
        warnings: Vec<OldWarning>,
    }

    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    let doc = source_dir.join("big.md");
    write_oversize_markdown(&doc);
    let catalog = bootstrap::open_catalog(&context).unwrap();
    let (card, _) =
        bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
    let source_id = orbok_core::SourceId::from_string(card.source_id.clone());
    orbok_fs::Scanner::new(&catalog)
        .scan(
            &orbok_fs::ScanRequest {
                source_id: source_id.clone(),
                force_hash: false,
                enqueue_index_jobs: false,
            },
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
    let file = FileRepository::new(&catalog)
        .find_by_canonical_path(&std::fs::canonicalize(&doc).unwrap().to_string_lossy())
        .unwrap()
        .expect("the scan registered the file");

    // The old entry, in the old namespace and the old shape.
    let old = localcache::CacheEngine::<OldOutput>::builder()
        .database(temp.path().join(orbok_db::CACHE_FILE_NAME))
        .namespace("extract-segments:v1")
        .payload_version(1)
        .change_detection(localcache::ChangeDetectionMode::MetadataThenFullHash)
        .compress()
        .build()
        .unwrap();
    old.set(
        std::fs::canonicalize(&doc).unwrap(),
        &OldOutput {
            extractor_name: "markdown".into(),
            extractor_version: "1".into(),
            normalization_version: "1".into(),
            segments: vec![],
            char_count: 0,
            warnings: vec![OldWarning::SizeLimitReached {
                limit_name: "segments".into(),
            }],
        },
    )
    .unwrap();

    IndexJobRepository::new(&catalog)
        .enqueue(JobType::Chunk, Some(&source_id), Some(&file.file_id))
        .unwrap();
    drain_scheduler_until_idle(&context, Duration::from_secs(60)).await;

    let failed_as_missing: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE job_type = 'chunk' AND status = 'failed' \
             AND error_category = 'extraction_cache_missing'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        failed_as_missing, 1,
        "the chunk job must find no entry under the current namespace and re-extract"
    );
    let results = search_marker(&context, &catalog, "realwarningmarker");
    assert_eq!(
        results.len(),
        1,
        "the file is searchable after re-extraction"
    );
    assert_eq!(results[0].trust.state, ResultTrustState::PartlyPrepared);
}

/// A PDF whose pages carry no text -- what a scanned document looks like to
/// the extractor -- built in code, so no binary is checked in.
fn write_text_less_pdf(path: &Path, pages: usize) {
    use lopdf::content::Content;
    use lopdf::{Document, Object, Stream, dictionary};
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids: Vec<Object> = Vec::new();
    for _ in 0..pages {
        let content = Content { operations: vec![] };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        kids.push(page_id.into());
    }
    let pages_dict = dictionary! {
        "Type" => "Pages",
        "Count" => kids.len() as i64,
        "Kids" => kids,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    doc.save(path).unwrap();
}

/// A one-page PDF with a real text-showing content stream -- the "gained
/// text" fixture for Task 080 test 5: the same file path a text-less PDF
/// used, rewritten with content a real PDF extractor reads back.
fn write_pdf_with_text(path: &Path, text: &str) {
    use lopdf::content::{Content, Operation};
    use lopdf::{Document, Object, Stream, dictionary};
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });
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
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
    });
    let pages_dict = dictionary! {
        "Type" => "Pages",
        "Count" => 1,
        "Kids" => vec![page_id.into()],
        "Resources" => resources_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages_dict));
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    doc.save(path).unwrap();
}

/// Task 077: a scanned PDF (pages, no text) is extracted with
/// `PossiblyScannedPdf`, and that warning survives the cache, so its chunk
/// job finishes instead of failing on the read-back.
///
/// This asserts the symptom the fix removes and nothing about the file's
/// final status: a text-less file has no chunks to search, and what state it
/// should end in is a separate question (Review Request 255 §5).
#[tokio::test]
async fn a_scanned_pdfs_warning_survives_the_cache_and_its_chunk_job_finishes() {
    use orbok_cache::{CacheService, OrbokCacheNamespace};
    use orbok_extract::{ExtractOutput, ExtractWarning};
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let pdf = source_dir.join("scan.pdf");
    write_text_less_pdf(&pdf, 2);
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(30)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let failed: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE status = 'failed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(failed, 0, "no job may fail because a warning was cached");

    let cache = bootstrap::cache_service(&context).unwrap();
    let engine = cache
        .engine::<ExtractOutput>(
            &catalog,
            &OrbokCacheNamespace::ExtractSegments,
            OrbokCacheNamespace::ExtractSegments.default_engine_options(),
        )
        .unwrap();
    let validated = orbok_search::snippet::searchable_path_guard(&catalog)
        .unwrap()
        .validate(&pdf)
        .unwrap();
    let cached = CacheService::get_fresh(&engine, &validated)
        .unwrap()
        .expect("the extraction is cached");
    assert_eq!(cached.warnings, vec![ExtractWarning::PossiblyScannedPdf]);
}

// ── Task 080: a file orbok read but found no text in is finished ────────

/// Task 080 test 1: a text-less PDF ends `NoTextFound`, not `discovered`,
/// with no job left queued or running.
#[tokio::test]
async fn a_text_less_pdf_finishes_with_no_text_found_and_no_pending_job() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let pdf = source_dir.join("scan.pdf");
    write_text_less_pdf(&pdf, 2);
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(30)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let status: String = catalog
        .lock()
        .query_row("SELECT file_status FROM files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        status, "no_text_found",
        "a text-less PDF must finish as no_text_found, not stay discovered"
    );
    let pending: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE status IN ('queued', 'running')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        pending, 0,
        "no job must be left pending for a finished file"
    );
}

/// Task 080 test 2: Task 078's startup repair does not re-queue a
/// `no_text_found` file -- its own SQL filter is `file_status =
/// 'discovered'`, and this file is no longer that. Through the real
/// startup entry point, so this exercises the actual repair, not the
/// filter in isolation.
#[tokio::test]
async fn startup_recovery_does_not_requeue_a_no_text_found_file() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let pdf = source_dir.join("scan.pdf");
    write_text_less_pdf(&pdf, 2);
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(30)).await;

    // "Restart": the real startup entry point, which runs Task 078's
    // repair among its other steps.
    let _state = bootstrap::load_initial_state(&context).unwrap();
    // Let the startup Scan job (RFC-037 SS10.1, queued for every source
    // regardless of file state) actually run, so a second-order requeue
    // from *it* would show up too, not just Task 078's own repair.
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    // A per-source Scan job is queued at every startup regardless of file
    // state (RFC-037 SS10.1) -- unrelated to Task 078's repair, and
    // `run_pending`/the scheduler treat Scan as a no-op. What matters here
    // is that no *extract* or *chunk* job was ever created for the
    // no_text_found file beyond the original scan's one of each -- not
    // just that none is still pending (a wrongly requeued extraction would
    // run and finish before this check, so "nothing pending" alone would
    // not catch a requeue that already completed).
    let extract_or_chunk_total: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE job_type IN ('extract', 'chunk')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        extract_or_chunk_total, 2,
        "a no_text_found file must not be requeued by the startup repair -- \
         exactly the original extract and chunk job, never a second round"
    );
    let status: String = catalog
        .lock()
        .query_row("SELECT file_status FROM files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "no_text_found", "the state must be left as it was");
}

/// Task 080 test 4: an ordinary file with real text is unaffected --
/// still `indexed`, still searchable, alongside a text-less file in the
/// same folder.
#[tokio::test]
async fn an_ordinary_file_is_unaffected_by_the_no_text_found_state() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    write_text_less_pdf(&source_dir.join("scan.pdf"), 2);
    write_markdown(
        &source_dir.join("notes.md"),
        "# Notes\n\nordinarymarker is here.\n",
    );
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(30)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let ordinary_status: String = catalog
        .lock()
        .query_row(
            "SELECT file_status FROM files WHERE display_path LIKE '%notes.md'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ordinary_status, "indexed");
    let results = search_marker(&context, &catalog, "ordinarymarker");
    assert_eq!(
        results.len(),
        1,
        "the ordinary file must still be searchable"
    );
}

/// Task 080 test 5: a `no_text_found` file that gains real text later
/// leaves the state and becomes searchable -- the state is not a dead
/// end. Rewrites the fixture with real content, then re-scans (the same
/// path a Check Folder or startup rescan takes for an edited file).
#[tokio::test]
async fn a_no_text_found_file_that_gains_text_later_becomes_searchable() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    let doc = source_dir.join("scan.pdf");
    write_text_less_pdf(&doc, 2);
    let source_id = {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
        card.source_id
    };
    drain_scheduler_until_idle(&context, Duration::from_secs(30)).await;
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let status: String = catalog
            .lock()
            .query_row("SELECT file_status FROM files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "no_text_found", "baseline: no text yet");
    }

    // The file changes: real text where there was none, same path (still
    // `.pdf`, so the same extractor applies) -- so the scanner sees a
    // genuine content change to the one file, not a new one.
    write_pdf_with_text(&doc, "gainedtextmarker is here");

    let catalog = bootstrap::open_catalog(&context).unwrap();
    bootstrap::check_and_refresh_source(&catalog, &source_id).unwrap();
    drop(catalog);
    drain_scheduler_until_idle(&context, Duration::from_secs(30)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let status: String = catalog
        .lock()
        .query_row("SELECT file_status FROM files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "indexed", "the state must not be a dead end");
    let results = search_marker(&context, &catalog, "gainedtextmarker");
    assert_eq!(results.len(), 1, "the new text must be searchable");
}

// ── Task 081: the Storage page shows what orbok really stores ───────────

/// Task 081 tests 1 and 2: on a real profile with an indexed file, the
/// total is non-zero, and at least two categories independently cross-
/// checked against a different read path than `measure_storage` itself
/// uses -- `persistent_catalog` against a direct `fs::metadata` call, and
/// `temporary_extraction` against a direct `CacheService::usage` call.
#[tokio::test]
async fn measuring_storage_reports_real_numbers_that_match_independent_reads() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    write_markdown(
        &source_dir.join("doc.md"),
        "# Doc\n\nreal text content for storage measurement.\n",
    );
    let catalog = bootstrap::open_catalog(&context).unwrap();
    let (card, _) =
        bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    drop(catalog);
    drain_scheduler_until_idle(&context, Duration::from_secs(30)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let (rows, cache_file_bytes) = bootstrap::measure_storage(&context, &catalog);

    let total: u64 = rows
        .iter()
        .filter(|(cat, _)| {
            !matches!(
                cat,
                orbok_core::StorageCategory::KeywordIndex
                    | orbok_core::StorageCategory::VectorIndex
            )
        })
        .filter_map(|(_, m)| match m {
            orbok_core::StorageMeasurement::Measured { bytes, .. } => Some(*bytes),
            orbok_core::StorageMeasurement::Unknown => None,
        })
        .sum();
    assert!(total > 0, "an indexed profile must report a non-zero total");
    assert!(
        cache_file_bytes.is_some_and(|b| b > 0),
        "the cache file itself must have a size once something is cached"
    );

    // persistent_catalog, cross-checked against an independent read.
    let catalog_path = temp.path().join(orbok_db::CATALOG_FILE_NAME);
    let expected_catalog_bytes = std::fs::metadata(&catalog_path).unwrap().len();
    let persistent = rows
        .iter()
        .find(|(cat, _)| *cat == orbok_core::StorageCategory::PersistentCatalog)
        .map(|(_, m)| *m)
        .unwrap();
    assert_eq!(
        persistent,
        orbok_core::StorageMeasurement::Measured {
            bytes: expected_catalog_bytes,
            items: 1,
        },
        "persistent_catalog must match a direct fs::metadata read of the catalog file, \
         and count the one registered file"
    );

    // temporary_extraction, cross-checked against a direct CacheService::usage call.
    let cache = bootstrap::cache_service(&context).unwrap();
    let expected_extraction = cache
        .usage(
            &catalog,
            &[orbok_cache::OrbokCacheNamespace::ExtractSegments],
        )
        .unwrap();
    let expected_bytes: u64 = expected_extraction.iter().map(|r| r.payload_bytes).sum();
    let expected_items: u64 = expected_extraction.iter().map(|r| r.entries).sum();
    let extraction = rows
        .iter()
        .find(|(cat, _)| *cat == orbok_core::StorageCategory::TemporaryExtraction)
        .map(|(_, m)| *m)
        .unwrap();
    assert_eq!(
        extraction,
        orbok_core::StorageMeasurement::Measured {
            bytes: expected_bytes,
            items: expected_items,
        },
        "temporary_extraction must match a direct CacheService::usage call"
    );
    assert!(
        expected_bytes > 0,
        "the one cached extraction must have real bytes, or this test proves nothing"
    );
}

/// Task 093 test 3: the Storage page still reports the `snippet_cache`
/// category from the catalog's own table, now that the `PreviewCache`
/// namespace half is gone. Seeds a real row directly (the same shape
/// `crates/data/db/src/tests.rs`'s own test uses -- nothing writes this
/// table in production, per Task 093's research) and cross-checks against
/// an independent `COUNT(*)` read.
#[tokio::test]
async fn snippet_cache_category_reports_the_catalog_table_alone() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let catalog = bootstrap::open_catalog(&context).unwrap();
    {
        let conn = catalog.lock();
        conn.execute(
            "INSERT INTO snippet_cache (snippet_id, snippet_text, created_at, \
             last_accessed_at, size_bytes) VALUES ('s1','some snippet text','t','t',18)",
            [],
        )
        .unwrap();
    }

    let (rows, _) = bootstrap::measure_storage(&context, &catalog);
    let snippet_cache = rows
        .iter()
        .find(|(cat, _)| *cat == orbok_core::StorageCategory::SnippetCache)
        .map(|(_, m)| *m)
        .unwrap();
    let expected_items: i64 = catalog
        .lock()
        .query_row("SELECT COUNT(*) FROM snippet_cache", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        expected_items, 1,
        "the one seeded row must be independently readable"
    );
    match snippet_cache {
        orbok_core::StorageMeasurement::Measured { items, bytes } => {
            assert_eq!(items, 1, "must count the one seeded catalog row");
            assert!(bytes > 0, "a real row must report non-zero bytes");
        }
        orbok_core::StorageMeasurement::Unknown => {
            panic!("snippet_cache must be measured, not Unknown, on a readable catalog")
        }
    }
}

/// Task 081 test 2 (retired-namespace fold-in): a Task 079 retired-
/// namespace row (`extract-segments:v1`) is counted inside
/// `temporary_extraction`, not left invisible the way it was before Task
/// 079 existed at all.
#[tokio::test]
async fn temporary_extraction_counts_retired_namespace_rows_too() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let catalog = bootstrap::open_catalog(&context).unwrap();
    let db_path = temp.path().join(orbok_db::CACHE_FILE_NAME);

    let stale_file = temp.path().join("stale.md");
    std::fs::write(&stale_file, "stale").unwrap();
    let retired = localcache::CacheEngine::<serde_json::Value>::builder()
        .database(&db_path)
        .namespace("extract-segments:v1".to_string())
        .change_detection(localcache::ChangeDetectionMode::MetadataThenFullHash)
        .build()
        .unwrap();
    retired
        .set(
            std::fs::canonicalize(&stale_file).unwrap(),
            &serde_json::json!({"stale": "x".repeat(1000)}),
        )
        .unwrap();
    drop(retired);

    let (rows, _) = bootstrap::measure_storage(&context, &catalog);
    let extraction = rows
        .iter()
        .find(|(cat, _)| *cat == orbok_core::StorageCategory::TemporaryExtraction)
        .map(|(_, m)| *m)
        .unwrap();
    match extraction {
        orbok_core::StorageMeasurement::Measured { bytes, items } => {
            assert!(
                bytes > 0 && items == 1,
                "the retired-namespace row must be counted: bytes={bytes}, items={items}"
            );
        }
        orbok_core::StorageMeasurement::Unknown => panic!("must be measured"),
    }
}

/// Task 081 test 3: a category that cannot be measured is `Unknown`, never
/// a silent zero -- and one unmeasurable category does not sink the rest.
/// The model store directory is replaced with a plain file, so `dir_size`'s
/// `read_dir` call fails deterministically (portable: reading a regular
/// file as a directory is an error on every platform this ships for).
#[tokio::test]
async fn an_unmeasurable_category_is_unknown_never_a_silent_zero() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let catalog = bootstrap::open_catalog(&context).unwrap();
    // Force the models directory path to be a file, not a directory.
    std::fs::write(temp.path().join("models"), b"not a directory").unwrap();

    let (rows, _cache_file_bytes) = bootstrap::measure_storage(&context, &catalog);

    let model_files = rows
        .iter()
        .find(|(cat, _)| *cat == orbok_core::StorageCategory::ModelFiles)
        .map(|(_, m)| *m)
        .unwrap();
    assert_eq!(
        model_files,
        orbok_core::StorageMeasurement::Unknown,
        "a directory that cannot be read must be Unknown, not a false zero"
    );
    let still_measured = rows
        .iter()
        .filter(|(cat, m)| {
            *cat != orbok_core::StorageCategory::ModelFiles
                && matches!(m, orbok_core::StorageMeasurement::Measured { .. })
        })
        .count();
    assert!(
        still_measured > 0,
        "one unmeasurable category must not sink every other category"
    );
}

/// Task 081 test 4 (first half): the extraction number drops to nothing
/// after *Clear extracted text*.
#[tokio::test]
async fn the_extraction_number_drops_after_clearing_extracted_text() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    write_markdown(&source_dir.join("doc.md"), "# Doc\n\nsome text.\n");
    let catalog = bootstrap::open_catalog(&context).unwrap();
    let (card, _) =
        bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    drop(catalog);
    drain_scheduler_until_idle(&context, Duration::from_secs(30)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let (before, _) = bootstrap::measure_storage(&context, &catalog);
    let before_bytes = match before
        .iter()
        .find(|(cat, _)| *cat == orbok_core::StorageCategory::TemporaryExtraction)
        .map(|(_, m)| *m)
        .unwrap()
    {
        orbok_core::StorageMeasurement::Measured { bytes, .. } => bytes,
        orbok_core::StorageMeasurement::Unknown => panic!("must be measured before clearing"),
    };
    assert!(
        before_bytes > 0,
        "baseline: something must be cached, or this test proves nothing"
    );

    let cache = bootstrap::cache_service(&context).unwrap();
    bootstrap::clean_temporary_extraction(&catalog, &cache).unwrap();

    let (after, _) = bootstrap::measure_storage(&context, &catalog);
    let after_bytes = match after
        .iter()
        .find(|(cat, _)| *cat == orbok_core::StorageCategory::TemporaryExtraction)
        .map(|(_, m)| *m)
        .unwrap()
    {
        orbok_core::StorageMeasurement::Measured { bytes, .. } => bytes,
        orbok_core::StorageMeasurement::Unknown => panic!("must still be measured after clearing"),
    };
    assert_eq!(
        after_bytes, 0,
        "clearing extracted text must drop the number to zero"
    );
}

/// Task 081 test 4 (second half): after a reset, a fresh measurement
/// reflects the real, now-empty state -- not the numbers from before the
/// reset.
#[tokio::test]
async fn numbers_reflect_reality_after_a_reset() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    write_markdown(&source_dir.join("doc.md"), "# Doc\n\nsome text.\n");
    let catalog = bootstrap::open_catalog(&context).unwrap();
    let (card, _) =
        bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    drop(catalog);
    drain_scheduler_until_idle(&context, Duration::from_secs(30)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let (before, _) = bootstrap::measure_storage(&context, &catalog);
    let files_before = match before
        .iter()
        .find(|(cat, _)| *cat == orbok_core::StorageCategory::PersistentCatalog)
        .map(|(_, m)| *m)
        .unwrap()
    {
        orbok_core::StorageMeasurement::Measured { items, .. } => items,
        orbok_core::StorageMeasurement::Unknown => panic!("must be measured before reset"),
    };
    assert_eq!(files_before, 1, "baseline: one registered file");

    let cache = bootstrap::cache_service(&context).unwrap();
    bootstrap::reset_catalog(&catalog, &cache).unwrap();

    let (after, _) = bootstrap::measure_storage(&context, &catalog);
    let files_after = match after
        .iter()
        .find(|(cat, _)| *cat == orbok_core::StorageCategory::PersistentCatalog)
        .map(|(_, m)| *m)
        .unwrap()
    {
        orbok_core::StorageMeasurement::Measured { items, .. } => items,
        orbok_core::StorageMeasurement::Unknown => panic!("must still be measured after reset"),
    };
    assert_eq!(
        files_after, 0,
        "a fresh measurement after reset must show zero registered files"
    );
}

/// Task 092/094 test 1: the reset confirmation's own counts match
/// independent `COUNT(*)` reads, taken separately from the code under
/// test -- two registered folders, a known number of indexed files, and
/// (Task 094) a known number of recorded searches.
#[tokio::test]
async fn reset_counts_match_independent_reads() {
    use orbok_core::SearchHistorySettings;
    use orbok_db::repo::{FileRepository, SearchHistoryRepository, SourceRepository};

    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let catalog = bootstrap::open_catalog(&context).unwrap();

    for name in ["one", "two"] {
        let dir = temp.path().join(name);
        write_markdown(&dir.join("doc.md"), "# Doc\n\nsome text.\n");
        let (card, _) = bootstrap::add_source_expect_added(&catalog, &dir.to_string_lossy())
            .unwrap_or_else(|_| panic!("{name} must be a newly added source"));
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    let history = SearchHistoryRepository::new(&catalog);
    for text in ["zephyrgraph", "marlinquartz"] {
        history
            .upsert(text, &[], Some(1), "en", &SearchHistorySettings::default())
            .unwrap();
    }
    drop(catalog);
    drain_scheduler_until_idle(&context, Duration::from_secs(30)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let counts = bootstrap::get_reset_counts(&catalog).unwrap();

    let independent_folders = SourceRepository::new(&catalog).count().unwrap();
    let independent_files = FileRepository::new(&catalog)
        .count_with_status(orbok_core::FileStatus::Indexed)
        .unwrap();
    let independent_history = SearchHistoryRepository::new(&catalog).count().unwrap() as u64;
    assert_eq!(counts.folders, independent_folders);
    assert_eq!(counts.files, independent_files);
    assert_eq!(counts.history, independent_history);
    assert_eq!(counts.folders, 2, "baseline: two registered folders");
    assert_eq!(counts.files, 2, "baseline: two indexed files");
    assert_eq!(counts.history, 2, "baseline: two recorded searches");
}

/// Task 092/094 test 6: after a real reset, the folder count really is
/// zero -- the number the dialog showed was the one actually removed, not
/// a number recomputed to match after the fact. Extended for history.
#[tokio::test]
async fn reset_counts_are_zero_after_a_real_reset() {
    use orbok_core::SearchHistorySettings;
    use orbok_db::repo::SearchHistoryRepository;

    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    write_markdown(&source_dir.join("doc.md"), "# Doc\n\nsome text.\n");
    let catalog = bootstrap::open_catalog(&context).unwrap();
    let (card, _) =
        bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
    bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    SearchHistoryRepository::new(&catalog)
        .upsert(
            "zephyrgraph",
            &[],
            Some(1),
            "en",
            &SearchHistorySettings::default(),
        )
        .unwrap();
    drop(catalog);
    drain_scheduler_until_idle(&context, Duration::from_secs(30)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let before = bootstrap::get_reset_counts(&catalog).unwrap();
    assert_eq!(before.folders, 1, "baseline: one registered folder");
    assert_eq!(before.files, 1, "baseline: one indexed file");
    assert_eq!(before.history, 1, "baseline: one recorded search");

    let cache = bootstrap::cache_service(&context).unwrap();
    bootstrap::reset_catalog(&catalog, &cache).unwrap();

    let after = bootstrap::get_reset_counts(&catalog).unwrap();
    assert_eq!(after.folders, 0, "a real reset must remove every folder");
    assert_eq!(after.files, 0, "a real reset must remove every file");
    assert_eq!(after.history, 0, "a real reset must remove every search");
}

/// Task 094 test 5: a failed history read fails the whole count, not just
/// its own clause -- `get_reset_counts` propagates every count with `?`
/// in one function, so a `search_history`-specific read failure (here,
/// the table itself is gone) must surface as `Err`, the same as a failed
/// folders or files read already does, never a line missing only the
/// history clause.
#[tokio::test]
async fn a_failed_history_read_fails_the_whole_count_not_just_its_clause() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let catalog = bootstrap::open_catalog(&context).unwrap();

    // Folders/files reads would still succeed; only history's own table
    // is gone -- proving this specific count's failure, not a general
    // catalog failure, is what must surface.
    catalog
        .lock()
        .execute("DROP TABLE search_history", [])
        .unwrap();

    assert!(
        bootstrap::get_reset_counts(&catalog).is_err(),
        "a history-specific read failure must fail the whole result"
    );
}

/// Task 081 test 5's classifier, in isolation: only "every category
/// Unknown and no cache-file size" counts as the whole measurement
/// failing.
#[test]
fn storage_measurement_is_failure_only_when_everything_is_unknown() {
    use orbok_core::{StorageCategory, StorageMeasurement};
    let all_unknown: Vec<_> = StorageCategory::ALL
        .iter()
        .map(|c| (*c, StorageMeasurement::Unknown))
        .collect();
    assert!(bootstrap::storage_measurement_is_failure(
        &all_unknown,
        None
    ));
    assert!(
        !bootstrap::storage_measurement_is_failure(&all_unknown, Some(1024)),
        "a readable cache file means the measurement was not a total failure"
    );

    let one_measured: Vec<_> = StorageCategory::ALL
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if i == 0 {
                (*c, StorageMeasurement::Measured { bytes: 1, items: 1 })
            } else {
                (*c, StorageMeasurement::Unknown)
            }
        })
        .collect();
    assert!(
        !bootstrap::storage_measurement_is_failure(&one_measured, None),
        "one measured category, out of eight, is a partial result, not a failure"
    );
}

/// RFC-038 §16 criterion 5 (HANDOFF-038 Slice 1), and Prepare again
/// end to end. A file edited after it was indexed, then seen by a refresh,
/// is `Needs update` with Prepare again offered -- and stays that way until
/// Prepare again queues it, after which it is Ready with its new content.
///
/// The refresh's own `Scan` job is left queued, and the scan it would run is
/// done by hand with no extraction queued: the hosted scheduler would
/// re-extract at once, so the stale window -- the one a user searches in --
/// would never be observable.
#[tokio::test]
async fn a_changed_file_needs_an_update_until_prepare_again_makes_it_ready() {
    use orbok_search::{ResultRecoveryAction, ResultTrustState};
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    let doc = source_dir.join("notes.md");
    write_markdown(&doc, "# Notes\n\noldcontentmarker is here.\n");
    let source_id = {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
        card.source_id
    };
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    assert_eq!(
        search_marker(&context, &catalog, "oldcontentmarker")[0]
            .trust
            .state,
        ResultTrustState::Ready,
        "baseline: indexed and Ready"
    );

    // Edit, then refresh (queues a Scan), then the scan's effect by hand.
    write_markdown(&doc, "# Notes\n\nnewcontentmarker is here.\n");
    bootstrap::check_and_refresh_source(&catalog, &source_id).unwrap();
    orbok_fs::Scanner::new(&catalog)
        .scan(
            &orbok_fs::ScanRequest {
                source_id: orbok_core::SourceId::from_string(source_id.clone()),
                force_hash: true,
                enqueue_index_jobs: false,
            },
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();

    let results = search_marker(&context, &catalog, "oldcontentmarker");
    assert_eq!(
        results.len(),
        1,
        "the file is still found by what it used to say"
    );
    assert_eq!(results[0].trust.state, ResultTrustState::NeedsUpdate);
    assert_eq!(
        results[0].trust.recovery_actions.first(),
        Some(&ResultRecoveryAction::PrepareAgain),
        "Prepare again is offered first, got {:?}",
        results[0].trust.recovery_actions
    );

    // Control: the scheduler running the refresh's Scan does not re-extract
    // it -- the file is still Needs update.
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;
    assert_eq!(
        search_marker(&context, &catalog, "oldcontentmarker")[0]
            .trust
            .state,
        ResultTrustState::NeedsUpdate,
        "nothing re-prepared it"
    );

    // Prepare again.
    let mut state = orbok_ui::AppState::default();
    state.update(&orbok_ui::state::Message::SearchResultsReady(
        search_marker(&context, &catalog, "oldcontentmarker"),
    ));
    let request = orbok_ui::state::Message::TrustRecoveryAction {
        result_idx: 0,
        action: ResultRecoveryAction::PrepareAgain,
    };
    crate::trust_actions::recover(
        &catalog,
        &mut state,
        0,
        ResultRecoveryAction::PrepareAgain,
        &request,
    );
    assert_eq!(state.notice, None);
    assert_eq!(
        state.search_results[0].trust.state,
        ResultTrustState::StillBeingPrepared
    );
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let fresh = search_marker(&context, &catalog, "newcontentmarker");
    assert_eq!(fresh.len(), 1, "the new content is found");
    assert_eq!(
        fresh[0].trust.state,
        ResultTrustState::Ready,
        "and the file is Ready"
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
/// **The `#[should_panic]` wrapper is removed by RFC-060 Slice 2**, which is
/// the change RFC-058 §6 Group B named as the one that would remove it: all
/// four retrieval sites (unicode61, trigram, vector scan, enrichment lookup)
/// now join `sources` and filter on status in SQL, so a paused source
/// contributes nothing. Until then this ran on every push and failed loudly,
/// naming the criterion, rather than sitting `#[ignore]`d and unobserved.
#[tokio::test]
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
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
        card.source_id
    };
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let results = bootstrap::run_search(
            &catalog,
            None,
            None,
            "pausedsourcemarker",
            orbok_search::SearchMode::Auto,
            20,
            orbok_core::SearchScope::default(),
        )
        .unwrap();
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
    let results = bootstrap::run_search(
        &catalog,
        None,
        None,
        "pausedsourcemarker",
        orbok_search::SearchMode::Auto,
        20,
        orbok_core::SearchScope::default(),
    )
    .unwrap();
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
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
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
        let results = bootstrap::run_search(
            &catalog,
            None,
            None,
            "temporarilygonemarker",
            orbok_search::SearchMode::Auto,
            20,
            orbok_core::SearchScope::default(),
        )
        .unwrap();
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
    let results = bootstrap::run_search(
        &catalog,
        None,
        None,
        "temporarilygonemarker",
        orbok_search::SearchMode::Auto,
        20,
        orbok_core::SearchScope::default(),
    )
    .unwrap();
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
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
        card.source_id
    };
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let results = bootstrap::run_search(
            &catalog,
            None,
            None,
            "unmountedfoldermarker",
            orbok_search::SearchMode::Auto,
            20,
            orbok_core::SearchScope::default(),
        )
        .unwrap();
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
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let results = bootstrap::run_search(
        &catalog,
        None,
        None,
        "認証エラー",
        orbok_search::SearchMode::Auto,
        20,
        orbok_core::SearchScope::default(),
    )
    .unwrap();
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

    let settings = bootstrap::load_runtime_settings(&context).unwrap();
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
        // The hosted scheduler needs its own resolved model to actually run
        // `GenerateEmbedding` jobs against (RFC-008 §15's `model_missing`
        // fallback otherwise means no vector ever gets written, regardless
        // of what `run_search` is later given) -- resolved separately from
        // the one below since `EmbeddingWorkerParts` isn't `Clone` and this
        // one is moved into the spawned scheduler task.
        let indexing_model = bootstrap::embedding_resolution::resolve_embedding_worker_parts(
            &context,
            &AllowRuntimePathProbe,
            &catalog,
            &settings,
        )
        .expect("RFC013_MODEL_DIR must resolve to a loadable embedding model");
        drain_scheduler_until_idle_with_embedding(
            &context,
            Duration::from_secs(60),
            Some(indexing_model),
        )
        .await;
    }

    let catalog = bootstrap::open_catalog(&context).unwrap();
    // RFC-061 §6 Slice 4: resolve once, the same way `main.rs` now does,
    // and reuse the same `EmbeddingWorkerParts` across every call below --
    // this loop is exactly the "hold for the loop's lifetime" shape the
    // RFC asks for. `resolve_embedding_worker_parts` finds-or-registers by
    // model name/version (`ensure_embedding_model_registered`), so this
    // gets back the identical catalog `ModelId` the indexing resolution
    // above registered, even though it is a second, separate model load.
    let model = bootstrap::embedding_resolution::resolve_embedding_worker_parts(
        &context,
        &AllowRuntimePathProbe,
        &catalog,
        &settings,
    )
    .expect("RFC013_MODEL_DIR must resolve to a loadable embedding model");
    let first = bootstrap::run_search(
        &catalog,
        Some(&model),
        None,
        "authentication token rotation",
        orbok_search::SearchMode::Auto,
        20,
        orbok_core::SearchScope::default(),
    )
    .unwrap();
    assert!(!first.is_empty(), "the corpus must be findable at all");
    // RFC-061 §6 Slice 4 mutation check: before this slice, `run_search`
    // passed `HybridSearchService::with_model` the model's constant
    // `model_name` as the vector lookup key, while the embedding worker
    // writes vectors under a catalog-registered `ModelId` -- the two never
    // matched, so `ExactVectorSearch` silently found zero rows and every
    // result's badges were keyword-only, no matter how well the corpus
    // matched semantically. Reverting `parts.model_id.as_str()` back to
    // `config.model_name` in `search.rs` makes this assertion fail.
    assert!(
        first
            .iter()
            .any(|r| r.badges.contains(&orbok_search::MatchBadge::Semantic)),
        "hybrid search must find at least one vector candidate under the shared \
         model_id -- if this fails, search and the embedding worker have drifted \
         back onto two different model_id keys"
    );
    let first_order: Vec<String> = first.iter().map(|r| r.display_path.clone()).collect();

    for i in 1..20 {
        let repeat = bootstrap::run_search(
            &catalog,
            Some(&model),
            None,
            "authentication token rotation",
            orbok_search::SearchMode::Auto,
            20,
            orbok_core::SearchScope::default(),
        )
        .unwrap();
        let order: Vec<String> = repeat.iter().map(|r| r.display_path.clone()).collect();
        assert_eq!(
            order, first_order,
            "search {i} returned a different order than search 0 -- \
             identical searches must return identical result orders"
        );
    }
}

/// Task 053: the Advanced search-mode selector reaches the engine.
///
/// No model is needed, and that is the point of the fixture: on a
/// keyword-only service, `Conceptual` disables the keyword half and has no
/// vector half to replace it, so the only way it returns nothing for a term
/// `Auto` finds is that the mode actually reached `HybridSearchService`.
/// `Exact` keeps the keyword half, so it must still find the term.
#[tokio::test]
async fn the_selected_search_mode_reaches_the_engine() {
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join("note.md"),
        "# Note\n\nsearchmodemarker in a markdown note.\n",
    )
    .unwrap();

    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let count_for = |mode: orbok_search::SearchMode| -> usize {
        bootstrap::run_search(
            &catalog,
            None,
            None,
            "searchmodemarker",
            mode,
            20,
            orbok_core::SearchScope::default(),
        )
        .unwrap()
        .len()
    };

    assert!(
        count_for(orbok_search::SearchMode::Auto) > 0,
        "Auto must find the indexed term"
    );
    assert!(
        count_for(orbok_search::SearchMode::Exact) > 0,
        "Exact keeps keyword retrieval, so it must find the term too"
    );
    assert_eq!(
        count_for(orbok_search::SearchMode::Conceptual),
        0,
        "Conceptual disables keyword retrieval; with no model it must return \
         nothing -- results here mean the selected mode never reached the engine"
    );
}

/// HANDOFF-041 §4 test 1: a found document can be opened -- through the
/// real search path, the index-carrying message, and the guard, reaching
/// the launcher once with that result's own canonical path. The rendered
/// button sending `OpenResult(i)` is `orbok-ui`'s
/// `the_selected_result_offers_open_and_show_in_folder`.
#[tokio::test]
async fn opening_a_found_result_launches_its_canonical_path_once() {
    use crate::result_launch::{LaunchAction, launch_result, tests::RecordingLauncher};
    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    let source_dir = temp.path().join("source");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::write(
        source_dir.join("note.md"),
        "# Note\n\nopenresultmarker here.\n",
    )
    .unwrap();
    {
        let catalog = bootstrap::open_catalog(&context).unwrap();
        let (card, _) =
            bootstrap::add_source_expect_added(&catalog, &source_dir.to_string_lossy()).unwrap();
        bootstrap::scan_and_index_source(&catalog, &card.source_id).unwrap();
    }
    drain_scheduler_until_idle(&context, Duration::from_secs(20)).await;

    let catalog = bootstrap::open_catalog(&context).unwrap();
    let results = bootstrap::run_search(
        &catalog,
        None,
        None,
        "openresultmarker",
        orbok_search::SearchMode::Auto,
        20,
        orbok_core::SearchScope::default(),
    )
    .unwrap();
    assert_eq!(results.len(), 1, "fixture: one result");

    let mut state = orbok_ui::AppState::default();
    state.update(&orbok_ui::Message::SearchResultsReady(results));
    state.update(&orbok_ui::Message::SelectResult(0));
    let Some(index) = state.selected_result else {
        panic!("the result is selected")
    };

    let launcher = RecordingLauncher::default();
    assert_eq!(
        launch_result(
            &catalog,
            &state.search_results,
            index,
            LaunchAction::Open,
            &launcher
        ),
        None
    );
    let expected = std::fs::canonicalize(source_dir.join("note.md")).unwrap();
    assert_eq!(
        launcher.0.borrow().as_slice(),
        [(LaunchAction::Open, expected)],
        "exactly one launch, with the found file's canonical path"
    );
}
