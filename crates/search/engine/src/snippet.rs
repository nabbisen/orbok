//! Dynamic snippet loading (FR-091): reads the relevant lines from the
//! original source file rather than storing extracted text permanently.
//!
//! Privacy: no text is stored in the catalog. Snippets surface only
//! when the source file is readable and current.

use orbok_core::{OrbokError, OrbokResult};
use orbok_db::Catalog;
use orbok_db::repo::ChunkRecord;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Load a text snippet for one chunk from its source file, using the
/// stored line range. Returns `None` when the source file is missing
/// or unreadable (UI should show "source unavailable").
pub fn load_snippet(record: &ChunkRecord, source_path: &str) -> Option<String> {
    let path = Path::new(source_path);
    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    let start = record.line_start.max(1) as usize;
    let end = record.line_end as usize;
    let max_lines = 8usize;
    let take = (end - start + 1).min(max_lines);

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
        Some(snippet.chars().take(400).collect())
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
    let sql = format!(
        "SELECT c.chunk_id, c.file_id, c.chunk_ordinal, c.heading_path, \
                cl.line_start, cl.line_end, cl.byte_start, cl.byte_end, cl.location_quality, \
                f.canonical_path \
         FROM chunks c \
         LEFT JOIN chunk_locations cl ON cl.chunk_id = c.chunk_id \
         JOIN files f ON f.file_id = c.file_id \
         WHERE c.chunk_id IN ({placeholders}) AND c.chunk_status = 'active'"
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
        },
        row.get::<_, String>(9)?,
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
