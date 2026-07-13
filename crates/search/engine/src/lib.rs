//! # orbok-search
//!
//! Retrieval layer, milestone M6 scope: the [`KeywordSearchEngine`]
//! trait (RFC-007 §6) and its SQLite FTS5 implementation over the
//! contentless `chunk_fts` table.
//!
//! Design properties:
//! - **no retrievable text**: the index is contentless (RFC-007 §8.1) —
//!   matching works, but no stored document text can be read back;
//!   display snippets load dynamically from source files via
//!   `chunk_locations`;
//! - **engine behind a trait**: Tantivy or another engine can replace
//!   FTS5 later (RFC-007 §6) without touching callers;
//! - **safe query building**: user input is converted into quoted FTS5
//!   phrase terms, never spliced into MATCH syntax (RFC-015 §13).
//!
//! Japanese segmentation is explicitly deferred to RFC-014: unicode61
//! treats a CJK run as a single token, so exact runs match but partial
//! Japanese terms do not. The keyword strategy RFC owns that gap.

pub mod filter;
mod fts5;
pub mod hybrid;
pub mod multilingual;
mod query;
pub mod result_trust;
pub mod rrf;
pub mod service;
pub mod snippet;
pub mod vector;

#[cfg(test)]
mod tests;

pub use filter::{
    ActiveFilter, ChangedFilter, KindFilter, LanguageFilter, ReadyFilter, SearchStyle,
    SuggestedFilter,
};
pub use fts5::Fts5KeywordEngine;
pub use hybrid::{HybridSearchService, SearchMode, SearchProfile, SearchTiming};
pub use multilingual::{MultilingualKeywordEngine, contains_cjk, normalize_query};
pub use query::build_match_expression;
pub use result_trust::{
    ResultRecoveryAction, ResultTrustState, ResultWarningSummary, SearchResultTrust,
};
pub use rrf::{FusedCandidate, rrf_fuse};
pub use service::{MatchBadge, SearchResult, SearchService};
pub use vector::ExactVectorSearch;

use orbok_core::{ChunkId, FileId, OrbokResult};

/// One document handed to the keyword indexer (normalized chunk text,
/// RFC-007 §9). The text is consumed for indexing and never stored.
#[derive(Debug, Clone)]
pub struct KeywordDocument {
    pub chunk_id: ChunkId,
    pub title: Option<String>,
    pub heading_path: Option<String>,
    pub normalized_text: String,
}

/// One keyword retrieval candidate (RFC-007 §10): rank is 1-based;
/// score is the engine-native relevance (BM25; lower = better for
/// FTS5's bm25()). RRF fusion (RFC-009) consumes ranks, not scores.
#[derive(Debug, Clone)]
pub struct KeywordCandidate {
    pub chunk_id: ChunkId,
    pub file_id: FileId,
    pub rank: u32,
    pub score: f64,
}

/// The keyword engine boundary (RFC-007 §6).
pub trait KeywordSearchEngine {
    /// Index (or reindex) documents. Existing entries for the same
    /// chunk are replaced.
    fn index(&self, documents: &[KeywordDocument]) -> OrbokResult<()>;

    /// Remove chunks from the index.
    fn delete(&self, chunk_ids: &[ChunkId]) -> OrbokResult<()>;

    /// Retrieve the top `limit` candidates for a raw user query.
    fn search(&self, query: &str, limit: u32) -> OrbokResult<Vec<KeywordCandidate>>;
}
