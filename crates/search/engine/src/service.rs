//! Keyword search service (RFC-007 §10, M6 complete).
//!
//! `SearchService::search` retrieves keyword candidates from FTS5,
//! enriches each with file path, heading context, and a dynamically
//! loaded snippet, and returns a `Vec<SearchResult>` ordered by rank.

use crate::KeywordSearchEngine;
use crate::fts5::Fts5KeywordEngine;
use crate::snippet::{chunk_record_for, load_snippet};
use orbok_core::{ChunkId, FileId, OrbokResult};
use orbok_db::Catalog;

/// A match badge indicating why this result appeared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchBadge {
    Keyword,
    Semantic, // placeholder — M7
    Reranked, // placeholder — M11
    SourceStale,
}

/// One search result (RFC-007 §10, external design §7.3 result card).
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub chunk_id: ChunkId,
    pub file_id: FileId,
    /// Canonical path for "open file" action (FR-093).
    pub canonical_path: String,
    /// Short path for UI display.
    pub display_path: String,
    pub title: Option<String>,
    pub heading_path: Option<String>,
    /// Dynamically loaded snippet; None when the source is unavailable.
    pub snippet: Option<String>,
    pub keyword_rank: u32,
    pub keyword_score: f64,
    pub badges: Vec<MatchBadge>,
}

/// Keyword-only search service (vector fusion deferred to M8).
pub struct SearchService<'a> {
    catalog: &'a Catalog,
}

impl<'a> SearchService<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    /// Execute a keyword search and return enriched results.
    pub fn search(&self, query: &str, limit: u32) -> OrbokResult<Vec<SearchResult>> {
        let engine = Fts5KeywordEngine::new(self.catalog);
        let candidates = engine.search(query, limit)?;

        let mut results = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let enriched = self.enrich(candidate)?;
            if let Some(r) = enriched {
                results.push(r);
            }
        }
        Ok(results)
    }

    fn enrich(&self, candidate: crate::KeywordCandidate) -> OrbokResult<Option<SearchResult>> {
        let Some((chunk, canonical_path)) = chunk_record_for(self.catalog, &candidate.chunk_id)?
        else {
            return Ok(None);
        };

        let snippet = load_snippet(&chunk, &canonical_path);

        // Build a short display path (just the last two components).
        let display_path = short_display_path(&canonical_path);

        // Derive title from heading or file name.
        let title = chunk.heading_path.clone().or_else(|| {
            std::path::Path::new(&canonical_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        });

        Ok(Some(SearchResult {
            chunk_id: candidate.chunk_id,
            file_id: candidate.file_id,
            canonical_path,
            display_path,
            title,
            heading_path: chunk.heading_path,
            snippet,
            keyword_rank: candidate.rank,
            keyword_score: candidate.score,
            badges: vec![MatchBadge::Keyword],
        }))
    }
}

fn short_display_path(path: &str) -> String {
    let p = std::path::Path::new(path);
    let parts: Vec<_> = p.components().collect();
    if parts.len() <= 2 {
        return path.to_string();
    }
    let tail: std::path::PathBuf = parts[parts.len() - 2..].iter().collect();
    format!("…/{}", tail.display())
}
