//! Adaptive chunker (RFC-006 §7–§9; RFC-044 §14 boundary cleanup).
//!
//! Strategy (RFC-006 §22 decision):
//! - **Document chunk** (parent, ordinal 0): whole document, used for
//!   passage-level reranking context.
//! - **Section chunks** (children): one per heading section (Markdown),
//!   one per paragraph group (plain text), or one (other types).
//! - **Fallback split**: segments exceeding `MAX_CHARS` are split into
//!   overlapping windows (RFC-006 §8.3 token-window fallback).
//!
//! The output is [`ExtractedChunk`] — a DB-free type. The pipeline
//! layer (`orbok-workers`) maps it to `orbok_db::repo::ChunkSpec`.
//! This removes the `orbok-db` dependency from `orbok-extract`
//! (RFC-044 §14.6 boundary rule).

use crate::types::{
    ExtractOutput, ExtractedChunk, LocationKind, LocationQuality, SegmentKind,
    chunk_location_quality,
};

/// Characters per fallback window (approximate token proxy).
const MAX_CHARS: usize = 1200;
/// Overlap between fallback windows.
const OVERLAP_CHARS: usize = 120;

/// Chunk a single-file extraction result into a flat list of
/// [`ExtractedChunk`]s ready for the pipeline layer.
///
/// Index 0 is always the document-level parent chunk.
/// Indices 1..N are the child (leaf) chunks used for retrieval.
pub fn chunk(output: &ExtractOutput, file_display_name: &str) -> Vec<ExtractedChunk> {
    if output.segments.is_empty() {
        return vec![empty_document_chunk(file_display_name)];
    }
    let all_text: String = output
        .segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let first_line = output.segments.first().map(|s| s.line_start).unwrap_or(1);
    let last_line = output.segments.last().map(|s| s.line_end).unwrap_or(1);
    let doc_location_kind = output
        .segments
        .first()
        .map(|s| s.location_kind)
        .unwrap_or(LocationKind::Unknown);

    let parent_heading = output.segments.iter().find_map(|s| {
        if s.kind == SegmentKind::Heading {
            Some(s.text.trim().to_string())
        } else {
            None
        }
    });

    let parent = ExtractedChunk {
        chunk_kind: "document",
        chunk_ordinal: 0,
        heading_path: parent_heading,
        title: Some(file_display_name.to_string()),
        normalized_text: all_text,
        location_kind: doc_location_kind,
        line_start: first_line,
        line_end: last_line,
        byte_start: None,
        byte_end: None,
        // RFC-060 Amendment 1 §4a.2: derived from every segment the
        // document chunk spans (all of them), not a bare literal --
        // `location_kind` two lines above was already derived this way,
        // `location_quality` never was.
        location_quality: chunk_location_quality(
            output.segments.iter().map(|s| s.location_quality),
        ),
        parent_idx: None,
    };

    let mut chunks = vec![parent];

    let has_markdown_structure = output
        .segments
        .iter()
        .any(|s| s.kind == SegmentKind::Heading);

    if has_markdown_structure {
        append_markdown_sections(output, &mut chunks);
    } else {
        append_paragraph_chunks(output, &mut chunks);
    }

    // Assign sequential ordinals (parent is 0, children start at 1).
    for (i, chunk) in chunks.iter_mut().enumerate() {
        chunk.chunk_ordinal = i as u32;
    }
    chunks
}

/// One child chunk per heading section in a Markdown document.
fn append_markdown_sections(output: &ExtractOutput, chunks: &mut Vec<ExtractedChunk>) {
    struct Section {
        heading: String,
        heading_path: String,
        segments: Vec<usize>,
    }

    let mut sections: Vec<Section> = Vec::new();
    let mut current_heading = String::new();
    let mut heading_trail: Vec<String> = Vec::new();

    for (i, seg) in output.segments.iter().enumerate() {
        if seg.kind == SegmentKind::Heading {
            let level = seg
                .heading_path
                .as_deref()
                .map(|h| h.matches('>').count() + 1)
                .unwrap_or(1);
            heading_trail.truncate(level.saturating_sub(1));
            heading_trail.push(seg.text.trim().to_string());
            current_heading = seg.text.trim().to_string();
            let path = heading_trail.join(" > ");
            sections.push(Section {
                heading: current_heading.clone(),
                heading_path: path,
                segments: vec![i],
            });
        } else if let Some(section) = sections.last_mut() {
            section.segments.push(i);
        } else {
            sections.push(Section {
                heading: current_heading.clone(),
                heading_path: String::new(),
                segments: vec![i],
            });
        }
    }

    for section in sections {
        if section.segments.is_empty() {
            continue;
        }
        let first = &output.segments[*section.segments.first().unwrap()];
        let last = &output.segments[*section.segments.last().unwrap()];
        let text: String = section
            .segments
            .iter()
            .map(|&i| output.segments[i].text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if text.trim().is_empty() {
            continue;
        }
        if text.len() > MAX_CHARS {
            append_text_windows(
                &text,
                first.line_start,
                last.line_end,
                first.location_kind,
                Some(section.heading_path.clone()),
                chunks,
            );
        } else {
            chunks.push(ExtractedChunk {
                chunk_kind: "section",
                chunk_ordinal: 0,
                heading_path: Some(section.heading_path),
                title: Some(section.heading),
                normalized_text: text,
                location_kind: first.location_kind,
                line_start: first.line_start,
                line_end: last.line_end,
                byte_start: None,
                byte_end: None,
                location_quality: chunk_location_quality(
                    section
                        .segments
                        .iter()
                        .map(|&i| output.segments[i].location_quality),
                ),
                parent_idx: Some(0),
            });
        }
    }
}

/// One child chunk per paragraph (or paragraph group within limit).
fn append_paragraph_chunks(output: &ExtractOutput, chunks: &mut Vec<ExtractedChunk>) {
    let mut buf = String::new();
    let mut buf_start = 0u32;
    let mut buf_end = 0u32;
    let mut buf_kind = LocationKind::Unknown;
    // RFC-060 Amendment 1 §4a.2: combined over every segment folded into
    // the buffer below (not just the first, unlike `buf_kind`), so a
    // buffer spanning a worse-quality segment can't inherit an earlier
    // segment's better quality.
    let mut buf_quality: Option<LocationQuality> = None;

    let flush = |buf: &mut String,
                 start: u32,
                 end: u32,
                 kind: LocationKind,
                 quality: Option<LocationQuality>,
                 chunks: &mut Vec<ExtractedChunk>| {
        let text = buf.trim().to_string();
        if text.is_empty() {
            buf.clear();
            return;
        }
        if text.len() > MAX_CHARS {
            append_text_windows(&text, start, end, kind, None, chunks);
        } else {
            chunks.push(ExtractedChunk {
                chunk_kind: "paragraph",
                chunk_ordinal: 0,
                heading_path: None,
                title: None,
                normalized_text: text,
                location_kind: kind,
                line_start: start,
                line_end: end,
                byte_start: None,
                byte_end: None,
                location_quality: chunk_location_quality(quality),
                parent_idx: Some(0),
            });
        }
        buf.clear();
    };

    for seg in &output.segments {
        if buf_start == 0 {
            buf_start = seg.line_start;
            buf_kind = seg.location_kind;
        }
        buf_end = seg.line_end;
        buf_quality = Some(match buf_quality {
            None => seg.location_quality,
            Some(q) => q.combine(seg.location_quality),
        });
        buf.push_str(&seg.text);
        buf.push('\n');
        if buf.len() >= MAX_CHARS {
            flush(&mut buf, buf_start, buf_end, buf_kind, buf_quality, chunks);
            buf_start = seg.line_end + 1;
            buf_quality = None;
        }
    }
    if !buf.trim().is_empty() {
        flush(&mut buf, buf_start, buf_end, buf_kind, buf_quality, chunks);
    }
}

/// Split a long text into overlapping fallback windows (RFC-006 §8.3).
fn append_text_windows(
    text: &str,
    line_start: u32,
    line_end: u32,
    location_kind: LocationKind,
    heading_path: Option<String>,
    chunks: &mut Vec<ExtractedChunk>,
) {
    let chars: Vec<char> = text.chars().collect();
    let total_lines = (line_end - line_start).max(1);
    let mut start = 0usize;
    while start < chars.len() {
        let end = (start + MAX_CHARS).min(chars.len());
        let window_text: String = chars[start..end].iter().collect();
        let frac_start = start as f64 / chars.len() as f64;
        let frac_end = end as f64 / chars.len() as f64;
        let wl_start = line_start + (total_lines as f64 * frac_start) as u32;
        let wl_end = line_start + (total_lines as f64 * frac_end) as u32;
        chunks.push(ExtractedChunk {
            chunk_kind: "fallback",
            chunk_ordinal: 0,
            heading_path: heading_path.clone(),
            title: None,
            normalized_text: window_text.trim().to_string(),
            location_kind,
            line_start: wl_start,
            line_end: wl_end,
            byte_start: None,
            byte_end: None,
            // Deliberately not derived from the spanned segments' quality
            // (RFC-060 Amendment 1 §4a.2 / HANDOFF-060 slice 1 §3.2): the
            // windower's own boundaries (`wl_start`/`wl_end` above,
            // interpolated by character fraction) are approximate even
            // when every underlying segment is `Exact` -- a window never
            // aligns with a real segment boundary, so it cannot be more
            // precise than "approximate" regardless of its source.
            location_quality: "approximate",
            parent_idx: Some(0),
        });
        if end == chars.len() {
            break;
        }
        start = end.saturating_sub(OVERLAP_CHARS);
    }
}

fn empty_document_chunk(name: &str) -> ExtractedChunk {
    ExtractedChunk {
        chunk_kind: "document",
        chunk_ordinal: 0,
        heading_path: None,
        title: Some(name.to_string()),
        normalized_text: String::new(),
        location_kind: LocationKind::Unknown,
        line_start: 1,
        line_end: 1,
        byte_start: None,
        byte_end: None,
        location_quality: "unknown",
        parent_idx: None,
    }
}
