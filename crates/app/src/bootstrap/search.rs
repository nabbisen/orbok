//! Keyword/hybrid search execution.

use super::embedding_resolution::EmbeddingWorkerParts;
use orbok_core::OrbokResult;
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
pub(crate) fn run_search(
    catalog: &Catalog,
    model: Option<&EmbeddingWorkerParts>,
    query: &str,
    limit: u32,
) -> OrbokResult<Vec<orbok_ui::state::SearchResultDisplay>> {
    let results = if let Some(parts) = model {
        let service =
            HybridSearchService::with_model(catalog, parts.model.as_ref(), parts.model_id.as_str());
        service.search(query, orbok_search::SearchMode::Auto, limit)?
    } else {
        HybridSearchService::keyword_only(catalog).search(
            query,
            orbok_search::SearchMode::Auto,
            limit,
        )?
    };
    Ok(results
        .into_iter()
        .map(|r| orbok_ui::state::SearchResultDisplay {
            display_path: r.display_path,
            title: r.title,
            heading_path: r.heading_path,
            snippet: r.snippet,
            keyword_rank: r.keyword_rank,
            badges: r.badges,
            trust: orbok_ui::state::ResultTrustDisplay::default(),
        })
        .collect())
}
