//! Task 013 Phase 2: the blocking-time measurement, and the hard gate
//! (§4/§5) — "do not ship Phase 1 into the synchronous path if it blocks
//! for more than a few seconds."
//!
//! This test performs exactly what a fully-wired `scan_and_index_source`
//! would do (`resolve_model_dir` → `recommended_config_from_model_dir` →
//! `create_embedding_model` → find-or-register the model → `run_pending`
//! with `Some(&embed_worker)`) — reusing the real production types for
//! every step except the small `Scanner::scan`/`run_pending` orchestration
//! glue, which is duplicated inline rather than added to
//! `crates/app/src/bootstrap/sources.rs` or wired into `main.rs`. That is
//! deliberate, not an oversight: Task 013 §5 requires committing Phase 1
//! separately from merging it into the synchronous call path, and this
//! measurement is what decides whether that merge is safe. Nothing here
//! is production-reachable.
//!
//! `#[ignore]`d: needs the real multilingual-e5-small model on disk. Run
//! manually:
//!
//! ```sh
//! RFC013_MODEL_DIR=~/.local/share/orbok/models/multilingual-e5-small \
//!   cargo test -p orbok --bin orbok --features orbok-embed/tract --release \
//!   measure_scan_and_index_blocking_time_with_a_real_model -- --ignored --nocapture
//! ```

use crate::bootstrap::model_resolution::{ResolvedModelDir, resolve_model_dir};
use crate::bootstrap::open_catalog;
use orbok::runtime_context::{
    AllowRuntimePathProbe, PlatformRuntimePaths, RuntimeContext, RuntimeSelection,
};
use orbok_db::repo::{ModelRecord, ModelRepository, ModelRole, ModelStatus, NewModel};
use orbok_embed::{create_embedding_model, recommended_config_from_model_dir};
use orbok_fs::{ScanRequest, Scanner};
use orbok_workers::{ChunkAndIndexWorker, EmbeddingWorker, ExtractionWorker, run_pending};
use std::sync::atomic::AtomicBool;

fn test_context(data_dir: &std::path::Path) -> RuntimeContext {
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

fn write_settings_with_model_dir(data_dir: &std::path::Path, model_dir: &str) {
    let settings = crate::settings::OrbokSettings {
        embedding_model_dir: Some(model_dir.to_string()),
        ..Default::default()
    };
    crate::settings::save_settings(&data_dir.join("settings.json"), &settings).unwrap();
}

fn seed_markdown_docs(source_dir: &std::path::Path, count: usize) {
    std::fs::create_dir_all(source_dir).unwrap();
    for i in 0..count {
        std::fs::write(
            source_dir.join(format!("doc{i}.md")),
            format!(
                "# Document {i}\n\n\
                 ## Install\n\n\
                 Run the installer and follow the on-screen prompts carefully, \
                 checking each step against the release notes for this version.\n\n\
                 ## Configure\n\n\
                 Edit the configuration file to match your local environment, \
                 then restart the service so the new settings take effect.\n"
            ),
        )
        .unwrap();
    }
}

/// Task 012 §2.2 / Review 160 §3: find-or-register, so repeated
/// measurement runs (or a real long-lived profile) accumulate embeddings
/// under one stable `model_id` instead of a fresh row each time.
fn ensure_embedding_model_registered(
    catalog: &orbok_db::Catalog,
    config: &orbok_models::EmbeddingModelConfig,
) -> orbok_core::OrbokResult<orbok_core::ModelId> {
    let repo = ModelRepository::new(catalog);
    if let Some(existing) =
        repo.list_by_role(ModelRole::Embedding)?
            .into_iter()
            .find(|record: &ModelRecord| {
                record.model_name == config.model_name
                    && record.model_version == config.model_version
            })
    {
        return Ok(existing.model_id);
    }
    let record = repo.insert(NewModel {
        role: ModelRole::Embedding,
        model_name: config.model_name.clone(),
        model_version: config.model_version.clone(),
        local_path: None,
        license_summary: None,
        size_bytes: None,
        backend: Some("tract".to_string()),
        dimension: Some(config.dimension),
        status: ModelStatus::Available,
    })?;
    Ok(record.model_id)
}

#[test]
#[ignore = "requires the real embedding model on disk; see RFC013_MODEL_DIR"]
fn measure_scan_and_index_blocking_time_with_a_real_model() {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let model_dir = std::env::var("RFC013_MODEL_DIR").expect(
        "RFC013_MODEL_DIR must point at the multilingual-e5-small model directory \
         (e.g. ~/.local/share/orbok/models/multilingual-e5-small)",
    );

    // "At least a few hundred files" per Task 013 §4.
    const N_DOCS: usize = 400;

    let temp = tempfile::tempdir().unwrap();
    let context = test_context(temp.path());
    write_settings_with_model_dir(temp.path(), &model_dir);

    let catalog = open_catalog(&context).unwrap();
    let cache_service = orbok::runtime_storage::cache(&context).unwrap();
    let cache_service = cache_service.service();

    let source_dir = temp.path().join("source");
    seed_markdown_docs(&source_dir, N_DOCS);

    let (card, _) = crate::bootstrap::add_source(&catalog, &source_dir.to_string_lossy()).unwrap();
    let source_id = orbok_core::SourceId::from_string(card.source_id.clone());

    // Same resolution `bootstrap/search.rs:31-49` already does.
    let settings = crate::bootstrap::load_runtime_settings(&context).unwrap();
    let resolved_model =
        match resolve_model_dir(&context, &AllowRuntimePathProbe, &catalog, &settings) {
            Ok(resolved) => resolved,
            Err(error) => {
                tracing::warn!(category = %error, "managed model resolution failed closed");
                ResolvedModelDir {
                    _guard: None,
                    path: None,
                    provenance: None,
                }
            }
        };
    let dir = resolved_model
        .path
        .as_ref()
        .expect("RFC013_MODEL_DIR was configured and must resolve");
    let config = recommended_config_from_model_dir(dir);
    let model = create_embedding_model(&config).expect("real model must load");
    let model_id = ensure_embedding_model_registered(&catalog, &config).unwrap();
    let embed = EmbeddingWorker::with_model(&catalog, cache_service, model, model_id);

    let extract = ExtractionWorker::new(&catalog, cache_service);
    let chunk = ChunkAndIndexWorker::new(&catalog, cache_service);

    let start = std::time::Instant::now();
    Scanner::new(&catalog)
        .scan(
            &ScanRequest {
                source_id,
                force_hash: false,
                enqueue_index_jobs: true,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
    // limit generous enough for Extract -> Chunk -> Embedding per file.
    run_pending(&catalog, &extract, &chunk, Some(&embed), N_DOCS as u32 * 4).unwrap();
    let elapsed = start.elapsed();
    // resolved_model (and its RFC-050 _guard) must outlive run_pending --
    // it does, still in scope here.
    drop(resolved_model);

    let embeddings: i64 = catalog
        .lock()
        .query_row("SELECT COUNT(*) FROM embeddings", [], |row| row.get(0))
        .unwrap();
    assert!(embeddings > 0, "embeddings must be created");

    let failed_embedding_jobs: i64 = catalog
        .lock()
        .query_row(
            "SELECT COUNT(*) FROM index_jobs WHERE job_type = 'embedding' AND status = 'failed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        failed_embedding_jobs, 0,
        "a real, resolved model must not leave embedding jobs failed"
    );

    let per_doc_ms = elapsed.as_secs_f64() * 1000.0 / N_DOCS as f64;
    println!(
        "scan_and_index (real model, fully wired): {N_DOCS} docs in {elapsed:?} \
         ({per_doc_ms:.1} ms/doc, embeddings={embeddings})"
    );
    println!("Task 013 §4 extrapolation, using this run's measured per-doc cost:");
    for realistic_size in [361usize, 1000, 5000] {
        let projected_secs = per_doc_ms * realistic_size as f64 / 1000.0;
        println!("  {realistic_size} files -> ~{projected_secs:.1}s blocking");
    }
    println!(
        "Task 013 §4 hard gate: a few seconds. This run's {N_DOCS}-doc measurement: \
         {:.1}s.",
        elapsed.as_secs_f64()
    );
}
