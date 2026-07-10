# RFC-006: Adaptive Chunking and Location Metadata

**Project:** orbok  
**RFC:** 006  
**Title:** Adaptive Chunking and Location Metadata  
**Status:** Implemented (v0.2.0)
**Target Milestone:** M5  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines how `orbok` converts extracted document segments into searchable chunks.

Chunking is the bridge between document extraction and retrieval. Keyword search, vector search, snippet loading, result preview, and reranking all depend on stable chunk identity and source-location metadata.

The central decision is:

> `orbok` should use structure-aware chunking where possible, fallback paragraph/token chunking where necessary, and always store location quality explicitly.

---

## 2. Motivation

The original requirement asked for dynamic chunking and parent/child chunk structure. That is correct, but insufficiently precise for implementation.

`orbok` needs chunking that supports:

- exact keyword retrieval;
- semantic retrieval;
- source snippet loading;
- stale-source detection;
- parent context expansion;
- local reranking within model token limits;
- storage-efficient metadata;
- rebuildability after extractor/chunker changes.

Poor chunking will directly harm search quality. Overly large chunks reduce precision and exceed reranker limits. Overly small chunks lose context and make results noisy.

---

## 3. Goals

- Define chunk model and chunk lifecycle.
- Preserve useful document structure.
- Support parent/child chunks.
- Track exact or approximate source locations.
- Enable dynamic snippet loading from source files.
- Support keyword indexing and vector embedding.
- Support localcache acceleration without making cache authoritative.
- Define chunk replacement semantics during reindexing.

---

## 4. Non-Goals

- This RFC does not choose the keyword search engine.
- This RFC does not choose the embedding model.
- This RFC does not define UI page layout.
- This RFC does not implement OCR.
- This RFC does not guarantee exact PDF highlights.
- This RFC does not define final Japanese tokenization.

---

## 5. Inputs and Outputs

## 5.1. Inputs

The chunker receives:

```text
file_id
extraction_id
source content hash
extractor name/version
normalization version
extracted segments
source locations
file type
index mode
language hints
```

## 5.2. Outputs

The chunker produces:

```text
chunk records
chunk location records
parent-child relationships
chunk content hash
chunking warnings
optional cached chunk bundle
```

The chunker should emit text to downstream indexers but should not permanently store full chunk text in the authoritative catalog by default.

---

## 6. Chunk Types

Recommended `chunk_kind` values:

| Kind | Meaning | Typical Source |
|---|---|---|
| document | Whole short document | Short `.txt`, short `.md` |
| section | Heading-bounded section | Markdown, HTML, DOCX |
| paragraph | Paragraph-level chunk | Text, DOCX, HTML |
| page | Page-level chunk | PDF |
| code_block | Code or fenced block | Markdown, source code |
| table | Table or CSV row group | CSV, HTML table |
| fallback | Token/character fallback chunk | Any unstructured text |

---

## 7. Parent-Child Chunk Model

## 7.1. Purpose

Parent chunks provide context. Child chunks provide precision.

Example:

```text
Document
└── Section: Security
    ├── Paragraph chunk 1
    ├── Paragraph chunk 2
    └── Code block chunk
```

## 7.2. Recommended Use

| Operation | Preferred Chunk Level |
|---|---|
| Keyword index | child chunk plus selected parent metadata |
| Embedding | child chunk by default |
| Result card | child chunk |
| Preview context | parent section if available |
| Reranking | child chunk, optionally with compact parent heading/context |
| Open source | source file + location |

## 7.3. Short Documents

Short documents may use one chunk:

```text
document chunk = parent and child
```

## 7.4. Long Documents

Long documents should create parent sections and child chunks.

---

## 8. Chunk Size Policy

## 8.1. Default Token Targets

Initial recommended values:

| Mode | Target Tokens | Max Tokens | Overlap |
|---|---:|---:|---:|
| Space Saving | 192 | 384 | 32 |
| Balanced | 384 | 768 | 64 |
| High Accuracy | 512 | 1024 | 96 |

These values are not final benchmark results. They are initial implementation defaults.

## 8.2. Hard Limits

The chunker must enforce:

- maximum characters per chunk;
- maximum tokens per chunk;
- maximum chunks per file;
- maximum extracted text per file.

Large files should be chunked incrementally where practical.

---

## 9. Structure-Aware Chunking

## 9.1. Markdown

Use:

- heading hierarchy;
- paragraph boundaries;
- fenced code block boundaries;
- list boundaries where practical.

Store:

- heading path;
- line range;
- approximate byte/char range if available.

## 9.2. HTML

Use:

- title;
- headings;
- paragraphs;
- list items;
- table text.

Ignore:

- script;
- style;
- remote resources.

Location quality is usually approximate.

## 9.3. PDF

Use:

- page boundary as minimum parent unit;
- extracted text blocks where available;
- section inference only if reliable.

Location quality should usually be `page_only` or `approximate`.

Do not claim exact highlight unless extractor supports it.

## 9.4. DOCX

Use:

- heading styles;
- paragraphs;
- tables.

Location quality is paragraph-level approximate.

## 9.5. CSV

Use:

- header row;
- row groups;
- row number ranges.

Chunk strategy:

```text
small CSV:
    one document/table chunk
large CSV:
    row group chunks with header repeated in downstream text
```

## 9.6. Source Code

Use:

- line ranges;
- top-level symbols if parser support exists;
- fallback line groups otherwise.

Do not over-normalize symbols and punctuation.

---

## 10. Fallback Chunking

Fallback chunking applies when structure is missing or unreliable.

Recommended fallback:

```text
split by paragraph
merge until target token count
if paragraph too large:
    split by sentence or line
if still too large:
    split by token/character window with overlap
```

Fallback chunks must still store location quality.

---

## 11. Location Metadata

Each chunk must have a `chunk_locations` record.

Fields:

```text
chunk_id
byte_start
byte_end
char_start
char_end
page_start
page_end
line_start
line_end
location_quality
locator_json
```

## 11.1. Location Quality

| Quality | Meaning |
|---|---|
| exact | Byte/char/line range is reliable |
| approximate | Location is useful but not exact |
| page_only | Only page or coarse page range is known |
| unknown | No safe source location available |

## 11.2. locator_json

`locator_json` stores format-specific hints.

Examples:

```json
{
  "markdown_heading_path": ["Security", "Token Lifecycle"]
}
```

```json
{
  "pdf_page": 12,
  "text_block_index": 4
}
```

```json
{
  "csv_row_start": 120,
  "csv_row_end": 180
}
```

---

## 12. Chunk Hashing

Each chunk should have a content hash computed from normalized chunk text and relevant context.

Recommended hash:

```text
sha256(normalized_chunk_text + chunker_version + source_content_hash)
```

Purpose:

- detect changed chunk content;
- avoid re-embedding unchanged chunks;
- deduplicate cache payloads;
- validate localcache payloads.

---

## 13. Chunker Versioning

The chunker must have a version string.

Example:

```text
chunker_version = "chunker-v1"
```

A chunker version change may mark existing chunks stale.

Version changes are required when:

- chunk boundary logic changes;
- token counting changes;
- parent/child policy changes;
- location metadata representation changes;
- language-specific splitting changes.

---

## 14. localcache Integration

`localcache` may be used to cache chunk bundles.

Recommended namespace:

```text
chunk-bundle:v1
```

Recommended change detection:

```text
MetadataThenFullHash
```

Payload:

```rust
pub struct ChunkBundle {
    pub chunker_version: String,
    pub source_content_hash_hint: Option<String>,
    pub chunks: Vec<CachedChunk>,
}
```

Rules:

1. The authoritative chunk records remain in the `orbok` catalog.
2. A localcache chunk bundle may be used to accelerate reindexing.
3. Payload version must change when chunk bundle schema changes.
4. The cache service must reject paths outside approved sources.
5. Cleanup may remove chunk bundles without deleting catalog source settings.

---

## 15. Database Impact

RFC-002 already defines:

```sql
chunks
chunk_locations
```

This RFC adds semantic constraints:

- `chunks.chunk_status` must represent active/stale/deleted/failed.
- `chunk_locations.location_quality` must be meaningful.
- `heading_path` should be filled where useful.
- old chunks should not be deleted until replacement chunks are committed.

---

## 16. Reindexing Strategy

Use replace-on-success:

1. create new extraction record;
2. create new chunks in transaction;
3. build keyword/vector indexes or mark jobs;
4. mark new chunks active;
5. mark old chunks stale/deleted;
6. mark file indexed.

If chunking fails, keep previous active chunks if they exist.

---

## 17. Downstream Contracts

## 17.1. Keyword Search Contract

Keyword indexer receives:

```text
chunk_id
file_id
chunk text
heading/title context
location metadata
language hint
```

## 17.2. Embedding Contract

Embedding worker receives:

```text
chunk_id
embedding text
model id
chunk content hash
```

Embedding text may include compact heading context:

```text
Title: ...
Section: ...
Text: ...
```

## 17.3. Snippet Loader Contract

Snippet loader receives:

```text
chunk_id
file path
source content hash
location metadata
location quality
```

If exact location is unavailable, it loads approximate context or page-level preview.

---

## 18. UI Impact

The UI should display location quality indirectly.

Examples:

| Location Quality | UI Text |
|---|---|
| exact line range | `Lines 120–145` |
| page_only | `PDF page 12` |
| approximate | `Approximate section` |
| unknown | `Source location unavailable` |

The UI must not promise exact highlights for approximate or page-only sources.

---

## 19. Acceptance Criteria

- Chunker can create chunks from extracted text.
- Short documents can be indexed as one chunk.
- Long documents are split into multiple chunks.
- Parent-child relationships are represented.
- Each chunk has location metadata.
- Location quality is explicit.
- Chunker version is recorded.
- Chunk hash is recorded.
- Old chunks survive if new chunking fails.
- localcache chunk bundle use is optional and non-authoritative.

---

## 20. Testing Requirements

Required tests:

1. Short text becomes one document chunk.
2. Long text becomes multiple fallback chunks.
3. Markdown headings create section context.
4. PDF extraction creates page-level chunks.
5. CSV row groups preserve row range.
6. Source code chunks preserve line ranges.
7. Approximate locations do not claim exact quality.
8. Chunker version change marks affected chunks stale.
9. Chunk hash stable for identical normalized text.
10. localcache chunk bundle cache miss on payload version mismatch.
11. Rechunk failure preserves previous active chunks.

---

## 21. Unresolved Questions

- Should child chunks include parent heading text in stored hash?
- What tokenizer should be used for token count before model selection?
- Should code files receive parser-based chunking in v1?
- Should table chunks be embedded row-wise or table-wise?
- Should chunk bundles store text-bearing data by default?

---

## 22. Decision

Adopt structure-aware chunking with fallback paragraph/token windows, parent-child chunk support, explicit location quality, and non-authoritative localcache chunk bundle acceleration.
