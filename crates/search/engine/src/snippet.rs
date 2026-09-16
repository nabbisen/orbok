//! Dynamic snippet loading (FR-091): reads the relevant lines from the
//! original source file rather than storing extracted text permanently.
//!
//! Privacy: no text is stored in the catalog. Snippets surface only
//! when the source file is readable and current.

use orbok_cache::{CacheService, OrbokCacheNamespace};
use orbok_core::{OrbokError, OrbokResult, SEARCHABLE_SOURCE_STATUS_SQL};
use orbok_db::Catalog;
use orbok_db::repo::{ChunkRecord, SourceRepository};
use orbok_extract::{ExtractOutput, LocationKind};
use orbok_fs::{GuardedSource, PathGuard, ValidatedPath};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

/// Read cap applied before line-splitting (Task 034 §5, audit S-18):
/// `BufRead::lines()` allocates one `String` per line with no cap of its
/// own, so a file with no newline byte would otherwise materialize
/// entirely in memory to produce this function's single, short snippet.
const MAX_SNIPPET_READ_BYTES: u64 = 64 * 1024;

/// Display cap, shared by both rendering paths (read-from-file and
/// cached-segments) so a PDF page's snippet is no longer than a text
/// file's.
const MAX_SNIPPET_CHARS: usize = 400;

/// Read a snippet from the file itself, for a chunk whose stored
/// positions really are line numbers. The caller has already validated
/// the path through the boundary.
fn load_snippet_from_file(record: &ChunkRecord, validated: &ValidatedPath) -> Option<String> {
    // Task 034 §5 (audit F-03): PDF/DOCX/HTML chunks store `Approximate`
    // location quality -- their `line_start`/`line_end` are paragraph or
    // page ordinals, not literal text-file line numbers, so reading "that
    // line range" from the source file returns the wrong bytes entirely. A
    // missing snippet is honest; a binary excerpt presented as document
    // text is not. Interim guard only -- RFC-060 owns the real fix
    // (locating actual text for these formats). Must run before the file
    // is even opened -- `load_snippet_from` below has no path to guard on.
    // Task 034 §5 (audit F-03) kept this quality gate in front of the file
    // read; `location_kind` now decides which path runs at all, and this
    // stays as the narrower guard on the read itself.
    if record.location_quality != "exact" {
        return None;
    }

    // **TOCTOU, recorded rather than fixed** (RFC-060 §9 accepts it as
    // separate): the guard canonicalised and checked membership, then this
    // open happens by path, so a path swapped for a symlink in between
    // still escapes. "The boundary is TOCTOU" is defensible; "the boundary
    // is not called" was not.
    let file = std::fs::File::open(&validated.canonical).ok()?;
    load_snippet_from(record, file)
}

/// Everything the snippet path is allowed to read: the source boundary,
/// and the extraction cache for formats whose stored positions are not
/// line numbers (RFC-060 §6).
///
/// **The snippet path never extracts** (RFC-060 Amendment 3, owner
/// decision): with no cached segments for a result, the snippet is simply
/// absent and the result is still shown. Re-parsing the user's file at
/// query time was ruled out.
pub struct SnippetSource<'a> {
    guard: PathGuard,
    /// The catalog and cache travel together: the cache engine is opened
    /// against the catalog, so one without the other reads nothing.
    cached_extraction: Option<(&'a Catalog, &'a CacheService)>,
}

impl<'a> SnippetSource<'a> {
    /// `cache` is `None` for callers that have no cache handle (the
    /// keyword-only service in tests and benchmarks); non-`Lines` chunks
    /// then render no snippet rather than the wrong bytes.
    pub fn new(catalog: &'a Catalog, cache: Option<&'a CacheService>) -> OrbokResult<Self> {
        Ok(Self {
            guard: searchable_path_guard(catalog)?,
            cached_extraction: cache.map(|cache| (catalog, cache)),
        })
    }

    /// A source over an explicit guard and no cache, for tests that
    /// exercise the file-reading half without a catalog behind them.
    #[cfg(test)]
    pub(crate) fn from_guard(guard: PathGuard) -> Self {
        Self {
            guard,
            cached_extraction: None,
        }
    }

    /// The snippet for one chunk, or `None`. A rejected path is logged and
    /// rendered as "no snippet" rather than failing the whole search: a
    /// result whose file the boundary refuses is still a real result.
    pub fn snippet_or_none(&self, record: &ChunkRecord, source_path: &str) -> Option<String> {
        match self.load(record, source_path) {
            Ok(snippet) => snippet,
            Err(error) => {
                tracing::warn!(
                    %error,
                    path = source_path,
                    "no snippet: the source boundary rejected this path"
                );
                None
            }
        }
    }

    /// RFC-060 §6: **only `LocationKind::Lines` reads the raw file.** For
    /// pages, paragraphs and blocks the stored positions are page or
    /// paragraph ordinals, so reading "those lines" returns unrelated
    /// bytes -- PDF object syntax, DOCX XML, HTML markup. Those render
    /// from the cached extraction segments instead, and render nothing
    /// when the cache holds no entry.
    pub fn load(&self, record: &ChunkRecord, source_path: &str) -> OrbokResult<Option<String>> {
        let validated = self.guard.validate(Path::new(source_path))?;
        match LocationKind::parse(&record.location_kind) {
            LocationKind::Lines => Ok(load_snippet_from_file(record, &validated)),
            LocationKind::Pages | LocationKind::Paragraphs | LocationKind::Blocks => {
                self.cached_segment_text(record, &validated)
            }
            // Including every row written before migration 0008, whose
            // column is NULL: absence, not a guess.
            LocationKind::Unknown => Ok(None),
        }
    }

    fn cached_segment_text(
        &self,
        record: &ChunkRecord,
        validated: &ValidatedPath,
    ) -> OrbokResult<Option<String>> {
        let Some((catalog, cache)) = self.cached_extraction else {
            return Ok(None);
        };
        let engine = cache.engine::<ExtractOutput>(
            catalog,
            &OrbokCacheNamespace::ExtractSegments,
            OrbokCacheNamespace::ExtractSegments.default_engine_options(),
        )?;
        let Some(extraction) = CacheService::get_fresh(&engine, validated)? else {
            return Ok(None);
        };
        // The segments whose own position range overlaps this chunk's --
        // the same span arithmetic `embedding.rs` uses to rebuild a
        // chunk's text, in the same units, since both read the positions
        // the extractor wrote.
        let text: String = extraction
            .segments
            .iter()
            .filter(|segment| {
                segment.line_start <= record.line_end && segment.line_end >= record.line_start
            })
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        Ok(Some(trimmed.chars().take(MAX_SNIPPET_CHARS).collect()))
    }
}

/// A guard over exactly the sources a search may read from -- the same
/// searchable set the retrieval queries filter on
/// ([`orbok_core::SourceStatus::is_searchable`]), so a paused source's
/// files cannot be opened even if a candidate reached the snippet path
/// (RFC-060 §11 criterion 6).
pub fn searchable_path_guard(catalog: &Catalog) -> OrbokResult<PathGuard> {
    let sources = SourceRepository::new(catalog).list()?;
    Ok(PathGuard::new(
        sources
            .iter()
            .filter(|source| source.status.is_searchable())
            .map(GuardedSource::from_record)
            .collect(),
    ))
}

/// The read-and-extract half of `load_snippet`, taking any `Read` rather
/// than a path (Task 045: split out so a test can feed an unbounded
/// source and assert the 64 KiB cap by byte count, not by timing how long
/// an unbounded read would take against a real multi-hundred-megabyte
/// fixture).
pub(crate) fn load_snippet_from(record: &ChunkRecord, source: impl Read) -> Option<String> {
    let reader = BufReader::new(source.take(MAX_SNIPPET_READ_BYTES));

    let start = record.line_start.max(1) as usize;
    let end = record.line_end as usize;
    let max_lines = 8usize;
    // `end.saturating_sub(start)` rather than `end - start`: a stored
    // `line_end < line_start` (a corrupted or malformed location) must not
    // panic on `usize` underflow.
    let take = end.saturating_sub(start).saturating_add(1).min(max_lines);

    let lines: Vec<String> = reader
        .lines()
        .skip(start.saturating_sub(1))
        .take(take)
        .filter_map(|l| l.ok())
        .collect();

    if lines.is_empty() {
        None
    } else {
        let snippet = lines.join("\n");
        // Trim to a reasonable display length.
        Some(snippet.chars().take(MAX_SNIPPET_CHARS).collect())
    }
}

/// Look up chunk location metadata from the catalog.
pub fn chunk_record_for(
    catalog: &Catalog,
    chunk_id: &orbok_core::ChunkId,
) -> OrbokResult<Option<(ChunkRecord, String)>> {
    let mut records = chunk_records_for(catalog, std::slice::from_ref(chunk_id))?;
    Ok(records.remove(chunk_id.as_str()))
}

/// Look up chunk location metadata for several chunks in one catalog query.
pub fn chunk_records_for(
    catalog: &Catalog,
    chunk_ids: &[orbok_core::ChunkId],
) -> OrbokResult<HashMap<String, (ChunkRecord, String)>> {
    if chunk_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = std::iter::repeat_n("?", chunk_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    // RFC-060 §5: the enrichment lookup applies the same source-status
    // filter as retrieval, so a paused source yields no record to render
    // and therefore no path to open.
    let sql = format!(
        "SELECT c.chunk_id, c.file_id, c.chunk_ordinal, c.heading_path, \
                cl.line_start, cl.line_end, cl.byte_start, cl.byte_end, cl.location_quality, \
                cl.location_kind, f.canonical_path \
         FROM chunks c \
         LEFT JOIN chunk_locations cl ON cl.chunk_id = c.chunk_id \
         JOIN files f ON f.file_id = c.file_id \
         JOIN sources s ON s.source_id = f.source_id \
         WHERE c.chunk_id IN ({placeholders}) AND c.chunk_status = 'active' \
           AND s.status IN {SEARCHABLE_SOURCE_STATUS_SQL}"
    );
    let conn = catalog.lock();

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| OrbokError::Database(e.to_string()))?;
    let params = rusqlite::params_from_iter(chunk_ids.iter().map(|id| id.as_str()));
    let rows = stmt
        .query_map(params, row_to_chunk_record)
        .map_err(|e| OrbokError::Database(e.to_string()))?;

    let mut records = HashMap::with_capacity(chunk_ids.len());
    for row in rows {
        let (record, canonical_path) = row.map_err(|e| OrbokError::Database(e.to_string()))?;
        records.insert(
            record.chunk_id.as_str().to_string(),
            (record, canonical_path),
        );
    }
    Ok(records)
}

fn row_to_chunk_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<(ChunkRecord, String)> {
    Ok((
        ChunkRecord {
            chunk_id: orbok_core::ChunkId::from_string(row.get::<_, String>(0)?),
            file_id: orbok_core::FileId::from_string(row.get::<_, String>(1)?),
            chunk_ordinal: row.get::<_, i64>(2)? as u32,
            heading_path: row.get(3)?,
            line_start: row.get::<_, i64>(4).unwrap_or(1) as u32,
            line_end: row.get::<_, i64>(5).unwrap_or(1) as u32,
            byte_start: row.get::<_, Option<i64>>(6)?.map(|v| v as u64),
            byte_end: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
            location_quality: row.get(8).unwrap_or_else(|_| "unknown".to_string()),
            location_kind: row.get(9).unwrap_or_else(|_| "unknown".to_string()),
        },
        row.get::<_, String>(10)?,
    ))
}

/// Sanitize a snippet for safe display in the UI (RFC-015 §18, FR-091).
///
/// Escapes `< > & " '` so that snippet text rendered in the GUI cannot
/// be interpreted as HTML markup. This is a defense-in-depth measure;
/// the iced/snora renderer does not evaluate HTML from text widgets,
/// but the escaping ensures correctness regardless of rendering backend.
pub fn html_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 16);
    for c in raw.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}
