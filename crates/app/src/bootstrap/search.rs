//! Keyword/hybrid search execution.

use super::embedding_resolution::EmbeddingWorkerParts;
use orbok_core::{OrbokResult, SearchScope};
use orbok_db::Catalog;
use orbok_search::HybridSearchService;

/// Execute a keyword/hybrid search and convert results to UI structs.
/// Uses hybrid search (keyword + semantic) when `model` is `Some`
/// (RFC-008/009); keyword-only otherwise.
///
/// RFC-061 §6 Slice 4: this used to resolve its own model -- calling
/// `create_embedding_model` (a full model load off disk) on every single
/// search. The caller now resolves once, the same way `scheduler_host::run`
/// already does via `embedding_resolution::resolve_embedding_worker_parts`,
/// and passes a borrow in here for the resolved model's whole lifetime.
/// That consolidation also fixes a correctness bug this replaces: the old
/// code passed `HybridSearchService::with_model` the model's constant
/// `model_name` (e.g. `"multilingual-e5-small"`) as the vector lookup key,
/// but the embedding worker writes vectors under a catalog-registered
/// `ModelId` (a generated `model_<uuid>` string, from `ModelRepository::insert`)
/// -- the two never matched, so `ExactVectorSearch` always scanned for a
/// `model_id` no row ever had and silently returned zero vector candidates.
/// Hybrid mode degraded to keyword-only in practice, at the added cost of
/// loading a model and embedding the query for nothing. Passing
/// `parts.model_id` (the same registered id the write side uses) fixes
/// this.
/// The scope one search runs under, built from what the user actually
/// chose: the active kind filters and the selected folder with its
/// subfolder setting (RFC-060 §7, RFC-041 §15, RFC-045 §6.3).
///
/// A `Folder` filter chip stands in when no location is selected; the
/// chosen location wins when both exist, since it is the one carrying a
/// subfolder setting.
pub(crate) fn scope_from_ui(
    filters: &[orbok_search::ActiveFilter],
    location: Option<&orbok_ui::state::SearchLocation>,
) -> SearchScope {
    let mut extensions: Vec<String> = Vec::new();
    let mut folder_from_chip: Option<String> = None;
    for filter in filters {
        match filter {
            orbok_search::ActiveFilter::Kind { value, .. } => extensions.extend(
                value
                    .extensions()
                    .iter()
                    .map(|ext| (*ext).to_ascii_lowercase()),
            ),
            orbok_search::ActiveFilter::Folder { id, .. } => {
                folder_from_chip.get_or_insert_with(|| id.clone());
            }
            _ => {}
        }
    }
    extensions.sort();
    extensions.dedup();

    let folder = match location.and_then(|loc| loc.source_id().map(|id| (id, loc))) {
        Some((source_id, loc)) => Some(orbok_core::FolderScope {
            source_id: source_id.as_str().to_string(),
            limit_path: loc.limit_path().map(str::to_string),
            include_subfolders: loc.scope().includes_subfolders(),
        }),
        None => folder_from_chip.map(|source_id| orbok_core::FolderScope {
            source_id,
            limit_path: None,
            include_subfolders: true,
        }),
    };

    SearchScope { extensions, folder }
}

pub(crate) fn run_search(
    catalog: &Catalog,
    model: Option<&EmbeddingWorkerParts>,
    extraction_cache: Option<&orbok_cache::CacheService>,
    query: &str,
    mode: orbok_search::SearchMode,
    limit: u32,
    scope: SearchScope,
) -> OrbokResult<Vec<orbok_ui::state::SearchResultDisplay>> {
    // RFC-060 §6: without this handle a PDF/DOCX/HTML result renders no
    // snippet at all, since its stored positions are pages or paragraphs
    // and reading "those lines" from the file returns unrelated bytes.
    let mut service = if let Some(parts) = model {
        HybridSearchService::with_model(catalog, parts.model.as_ref(), parts.model_id.as_str())
    } else {
        HybridSearchService::keyword_only(catalog)
    };
    if let Some(cache) = extraction_cache {
        service = service.with_extraction_cache(cache);
    }
    // Task 053: `mode` is the Advanced selector's choice. This was a
    // hardcoded `SearchMode::Auto`, so Exact and Conceptual did nothing.
    // No fallback here: Conceptual without a model has no keyword half and
    // returns nothing -- the view is what stops a keyword-only install
    // from choosing it.
    let results = service
        .search_request(&orbok_search::SearchRequest::new(query, mode, limit).with_scope(scope))?;
    Ok(results
        .into_iter()
        .map(|r| orbok_ui::state::SearchResultDisplay {
            canonical_path: r.canonical_path,
            display_path: r.display_path,
            title: r.title,
            heading_path: r.heading_path,
            snippet: r.snippet,
            keyword_rank: r.keyword_rank,
            badges: r.badges,
            // RFC-060 §11 criterion 3: this was
            // `ResultTrustDisplay::default()` -- state `Ready`, no
            // recovery actions -- on every result, whatever the file
            // behind it was doing.
            trust: orbok_ui::state::ResultTrustDisplay {
                state: r.trust.state,
                recovery_actions: r.trust.recovery_actions,
                warnings: r.trust.warnings,
            },
        })
        .collect())
}
