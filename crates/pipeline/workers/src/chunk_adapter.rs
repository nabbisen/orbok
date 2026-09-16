//! Adapter: `ExtractedChunk` → `orbok_db::repo::ChunkSpec` (RFC-044 §14).
//!
//! This conversion lives in `orbok-workers` (not in `orbok-extract`)
//! so that the extraction crate has no dependency on `orbok-db`.
//! `location_kind` reaches the catalog from here: RFC-060 §6 / migration
//! 0008 gave `chunk_locations` a column for it, so the snippet path can
//! tell whether a chunk's `line_start`/`line_end` are line numbers at all
//! before reading them from the file as lines.

use orbok_db::repo::ChunkSpec;
use orbok_extract::ExtractedChunk;

/// Convert one `ExtractedChunk` to a `ChunkSpec`.
pub fn to_chunk_spec(c: ExtractedChunk) -> ChunkSpec {
    ChunkSpec {
        chunk_kind: c.chunk_kind,
        chunk_ordinal: c.chunk_ordinal,
        heading_path: c.heading_path,
        title: c.title,
        normalized_text: c.normalized_text,
        line_start: c.line_start,
        line_end: c.line_end,
        byte_start: c.byte_start,
        byte_end: c.byte_end,
        location_quality: c.location_quality,
        location_kind: c.location_kind.as_str(),
        parent_idx: c.parent_idx,
    }
}

/// Convert a `Vec<ExtractedChunk>` to a `Vec<ChunkSpec>`.
pub fn to_chunk_specs(chunks: Vec<ExtractedChunk>) -> Vec<ChunkSpec> {
    chunks.into_iter().map(to_chunk_spec).collect()
}
