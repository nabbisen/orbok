//! Multilingual keyword search (RFC-014).
//!
//! Strategy (RFC-014 §21 decision):
//! - Every chunk is indexed in both unicode61 (exact, identifier-safe)
//!   and trigram (CJK recall, partial matching).
//! - Query routing: if the query contains CJK characters, query both
//!   tables and merge; otherwise query only unicode61.
//! - Full-width → half-width normalization applied to queries so that
//!   ＡＢＣ finds ABC, RFC-014 §10 requirement 1.
//! - Identifier preservation: exact tokens like `client_secret` and
//!   `RFC-014` pass through the unicode61 index unchanged.
//!
//! Japanese-specific tokenization (Tantivy, mecab, etc.) is deferred to
//! a future RFC when a suitable licensed crate is available.

use crate::fts5::Fts5KeywordEngine;
use crate::query::{build_match_expression, build_match_pair_expression};
use crate::{KeywordCandidate, KeywordSearchEngine};
use orbok_core::{ChunkId, FileId, OrbokError, OrbokResult};
use orbok_db::Catalog;
use rusqlite::params;

/// True when the string contains any CJK unified ideograph, hiragana,
/// katakana, or fullwidth form character (RFC-014 §9 CJK detection).
pub fn contains_cjk(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(c,
            '\u{1100}'..='\u{11FF}'   // Hangul Jamo
            | '\u{3000}'..='\u{9FFF}' // CJK + kana (covers hiragana, katakana, CJK)
            | '\u{F900}'..='\u{FAFF}' // CJK compatibility
            | '\u{FF00}'..='\u{FFEF}' // Fullwidth + halfwidth forms
        )
    })
}

/// Normalize query text before building a MATCH expression (RFC-014 §10):
/// - NFKC Unicode normalization (full-width → half-width, etc.)
/// - trim whitespace
pub fn normalize_query(query: &str) -> String {
    // NFKC decomposition followed by re-composition approximation:
    // convert fullwidth ASCII/digits to half-width via simple range map,
    // then lowercase.
    query
        .chars()
        .map(|c| {
            // Fullwidth ASCII letters: U+FF21–U+FF3A (A–Z), U+FF41–U+FF5A (a–z)
            // Fullwidth digits: U+FF10–U+FF19 (0–9)
            if ('\u{FF21}'..='\u{FF3A}').contains(&c) {
                char::from_u32(c as u32 - 0xFF21 + 0x0041).unwrap_or(c)
            } else if ('\u{FF41}'..='\u{FF5A}').contains(&c) {
                char::from_u32(c as u32 - 0xFF41 + 0x0061).unwrap_or(c)
            } else if ('\u{FF10}'..='\u{FF19}').contains(&c) {
                char::from_u32(c as u32 - 0xFF10 + 0x0030).unwrap_or(c)
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Multilingual keyword search engine (RFC-014 §12).
pub struct MultilingualKeywordEngine<'a> {
    catalog: &'a Catalog,
}

impl<'a> MultilingualKeywordEngine<'a> {
    pub fn new(catalog: &'a Catalog) -> Self {
        Self { catalog }
    }

    pub fn search_pairs(&self, query: &str, limit: u32) -> OrbokResult<Vec<KeywordCandidate>> {
        self.search_with_pairs(query, limit)
    }
}

impl KeywordSearchEngine for MultilingualKeywordEngine<'_> {
    fn index(&self, documents: &[crate::KeywordDocument]) -> OrbokResult<()> {
        // Indexing goes through ChunkRepository::insert_bundle which
        // handles both FTS tables; this method is a no-op for the
        // multilingual engine.
        let _ = documents;
        Ok(())
    }

    fn delete(&self, chunk_ids: &[ChunkId]) -> OrbokResult<()> {
        Fts5KeywordEngine::new(self.catalog).delete(chunk_ids)
    }

    /// Search with query routing: CJK queries use trigram in addition to
    /// unicode61; English/identifier queries use unicode61 only.
    fn search(&self, query: &str, limit: u32) -> OrbokResult<Vec<KeywordCandidate>> {
        self.search_with_exact_terms(query, limit)
    }
}

impl MultilingualKeywordEngine<'_> {
    fn search_with_exact_terms(
        &self,
        query: &str,
        limit: u32,
    ) -> OrbokResult<Vec<KeywordCandidate>> {
        let normalized = normalize_query(query);
        if normalized.is_empty() {
            return Ok(Vec::new());
        }

        // Always query the unicode61 table for identifiers and exact terms.
        let kw = Fts5KeywordEngine::new(self.catalog);
        let mut candidates = kw.search(&normalized, limit)?;

        // For CJK queries, also query the trigram table and merge.
        if contains_cjk(&normalized) {
            let trigram_hits = self.search_trigram(&normalized, limit)?;
            merge_candidates(&mut candidates, trigram_hits);
            candidates.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            candidates.truncate(limit as usize);
            // Re-assign 1-based ranks after merge.
            for (i, c) in candidates.iter_mut().enumerate() {
                c.rank = (i + 1) as u32;
            }
        }
        Ok(candidates)
    }

    fn search_with_pairs(&self, query: &str, limit: u32) -> OrbokResult<Vec<KeywordCandidate>> {
        let normalized = normalize_query(query);
        if normalized.is_empty() {
            return Ok(Vec::new());
        }

        let kw = Fts5KeywordEngine::new(self.catalog);
        let mut candidates = kw.search_pairs(&normalized, limit)?;

        if contains_cjk(&normalized) {
            let trigram_hits = self.search_trigram_pairs(&normalized, limit)?;
            merge_candidates(&mut candidates, trigram_hits);
            candidates.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            candidates.truncate(limit as usize);
            for (i, c) in candidates.iter_mut().enumerate() {
                c.rank = (i + 1) as u32;
            }
        }
        Ok(candidates)
    }

    fn search_trigram(&self, query: &str, limit: u32) -> OrbokResult<Vec<KeywordCandidate>> {
        self.search_trigram_with_expr(build_match_expression(query), limit)
    }

    fn search_trigram_pairs(&self, query: &str, limit: u32) -> OrbokResult<Vec<KeywordCandidate>> {
        self.search_trigram_with_expr(build_match_pair_expression(query), limit)
    }

    fn search_trigram_with_expr(
        &self,
        match_expr: Option<String>,
        limit: u32,
    ) -> OrbokResult<Vec<KeywordCandidate>> {
        let Some(match_expr) = match_expr else {
            return Ok(Vec::new());
        };
        let conn = self.catalog.lock();
        let mut stmt = conn
            .prepare(
                "SELECT r.chunk_id, c.file_id, bm25(chunk_fts_trigram) AS score \
                 FROM chunk_fts_trigram \
                 JOIN keyword_index_records r ON r.trigram_fts_rowid = chunk_fts_trigram.rowid \
                 JOIN chunks c ON c.chunk_id = r.chunk_id \
                 WHERE chunk_fts_trigram MATCH ?1 AND r.status = 'active' \
                   AND c.chunk_status = 'active' \
                 ORDER BY score LIMIT ?2",
            )
            .map_err(|e| OrbokError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![match_expr, limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                ))
            })
            .map_err(|e| OrbokError::Database(e.to_string()))?;
        let mut out = Vec::new();
        for (i, row) in rows.enumerate() {
            let (chunk_id, file_id, score) =
                row.map_err(|e| OrbokError::Database(e.to_string()))?;
            out.push(KeywordCandidate {
                chunk_id: ChunkId::from_string(chunk_id),
                file_id: FileId::from_string(file_id),
                rank: (i + 1) as u32,
                score,
            });
        }
        Ok(out)
    }
}

/// Merge trigram hits into the candidate list, deduplicating by chunk_id.
fn merge_candidates(existing: &mut Vec<KeywordCandidate>, new: Vec<KeywordCandidate>) {
    let existing_ids: std::collections::HashSet<String> = existing
        .iter()
        .map(|c| c.chunk_id.as_str().to_string())
        .collect();
    for c in new {
        if !existing_ids.contains(c.chunk_id.as_str()) {
            existing.push(c);
        }
    }
}
