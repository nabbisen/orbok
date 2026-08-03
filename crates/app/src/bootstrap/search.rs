//! Keyword/hybrid search execution.

use super::model_resolution::{ResolvedModelDir, resolve_model_dir};
use orbok::runtime_context::{AllowRuntimePathProbe, RuntimeContext, RuntimePathProbe};
use orbok_db::Catalog;
use orbok_embed::{create_embedding_model, recommended_config_from_model_dir};
use orbok_models::EmbeddingModel;
use orbok_search::HybridSearchService;

/// Execute a keyword/hybrid search and convert results to UI structs.
/// Uses hybrid search (keyword + semantic) when an embedding model is
/// configured and the tract feature is compiled in; keyword-only
/// otherwise (RFC-008/009).
pub(crate) fn run_search(
    context: &RuntimeContext,
    catalog: &Catalog,
    query: &str,
    limit: u32,
) -> Result<Vec<orbok_ui::state::SearchResultDisplay>, Box<dyn std::error::Error>> {
    run_search_with(context, &AllowRuntimePathProbe, catalog, query, limit)
}

pub(crate) fn run_search_with<P: RuntimePathProbe + ?Sized>(
    context: &RuntimeContext,
    probe: &P,
    catalog: &Catalog,
    query: &str,
    limit: u32,
) -> Result<Vec<orbok_ui::state::SearchResultDisplay>, Box<dyn std::error::Error>> {
    let settings = super::runtime_settings_with(context, probe)?;
    let resolved_model = match resolve_model_dir(context, probe, catalog, &settings) {
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
    let results = if let Some(dir) = &resolved_model.path {
        let config = recommended_config_from_model_dir(dir);
        match create_embedding_model(&config) {
            Ok(model) => {
                // Real model available — use hybrid search.
                let model_ref: &dyn EmbeddingModel = model.as_ref();
                let service =
                    HybridSearchService::with_model(catalog, model_ref, &config.model_name);
                service.search(query, orbok_search::SearchMode::Auto, limit)?
            }
            Err(_) => {
                // Model configured but backend not compiled in (e.g. no --features tract).
                // Fall back to keyword-only.
                HybridSearchService::keyword_only(catalog).search(
                    query,
                    orbok_search::SearchMode::Auto,
                    limit,
                )?
            }
        }
    } else {
        // No model configured — keyword-only.
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
