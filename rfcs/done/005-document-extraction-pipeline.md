# RFC-005: Document Extraction Pipeline

**Project:** orbok  
**RFC:** 005  
**Title:** Document Extraction Pipeline  
**Status:** Implemented (v0.1.0)
**Target Milestone:** M4  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines the document extraction pipeline for `orbok`.

Extraction converts supported source files into normalized text streams and location hints that later stages can chunk, index, embed, and display.

The extraction pipeline must be versioned, failure-isolated, and privacy-conscious.

`localcache` may be used to cache expensive per-file extraction outputs, but text-bearing cache storage must remain visible, cleanable, and governed by privacy settings.

---

## 2. Motivation

`orbok` cannot search local documents unless it can extract text. However, document extraction is messy:

- PDFs may have unreliable reading order;
- DOCX structure may not map cleanly to byte offsets;
- HTML may contain scripts, menus, and boilerplate;
- CSV may require row-aware extraction;
- source code should preserve symbols and line numbers;
- parser failures must not stop the whole index.

The extraction layer must provide enough information for downstream chunking and snippet loading without permanently storing full extracted text by default.

---

## 3. Goals

- Define extractor pipeline architecture.
- Support initial text-oriented file types.
- Record extractor and normalization versions.
- Preserve useful source location metadata.
- Isolate extraction failures per file.
- Avoid permanent full extracted-text storage by default.
- Provide temporary extraction buffers for downstream indexing.
- Support re-extraction when extractor behavior changes.

---

## 4. Non-Goals

- This RFC does not define final chunking strategy.
- This RFC does not implement keyword search.
- This RFC does not implement embeddings.
- This RFC does not implement OCR.
- This RFC does not guarantee pixel-perfect PDF highlighting.
- This RFC does not parse every proprietary document format.

---

## 5. Supported Initial File Types

Recommended initial support:

| Type | Extensions | Extraction Approach | Location Quality |
|---|---|---|---|
| Plain text | `.txt`, `.log` | streaming text read | exact line/byte |
| Markdown | `.md`, `.markdown` | text read + heading hints | line/heading |
| HTML | `.html`, `.htm` | sanitized text extraction | approximate selector/line |
| PDF | `.pdf` | text extraction by page | page-level/approximate |
| DOCX | `.docx` | document XML extraction | paragraph-level approximate |
| CSV | `.csv` | row-aware text extraction | row-level |
| Source code | common code extensions | line-aware text read | exact line |

Unsupported formats should be recorded as `unsupported`, not treated as fatal errors.

---

## 6. Extractor Trait

Recommended conceptual interface:

```rust
pub trait DocumentExtractor {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn supports(&self, file: &FileDescriptor) -> bool;
    fn extract(&self, input: ExtractInput) -> Result<ExtractOutput, ExtractError>;
}
```

## 6.1. ExtractInput

```rust
pub struct ExtractInput {
    pub file_id: FileId,
    pub path: PathBuf,
    pub content_hash: Option<String>,
    pub max_bytes: u64,
    pub cancellation: CancellationToken,
}
```

## 6.2. ExtractOutput

```rust
pub struct ExtractOutput {
    pub segments: Vec<ExtractedSegment>,
    pub char_count: u64,
    pub byte_count: u64,
    pub warnings: Vec<ExtractWarning>,
}
```

## 6.3. ExtractedSegment

```rust
pub struct ExtractedSegment {
    pub text: String,
    pub segment_kind: SegmentKind,
    pub location: SourceLocation,
    pub title_hint: Option<String>,
}
```

---

## 7. Segment Kinds

Recommended segment kinds:

```text
document
section
paragraph
page
row
code_block
line_group
fallback
```

These are not final chunks. They are extraction-level units that the chunker may later combine or split.

---

## 8. Source Location

Recommended source location model:

```rust
pub struct SourceLocation {
    pub byte_range: Option<Range<u64>>,
    pub char_range: Option<Range<u64>>,
    pub line_range: Option<Range<u64>>,
    pub page_range: Option<Range<u64>>,
    pub locator_json: Option<String>,
    pub quality: LocationQuality,
}
```

Location quality:

```text
exact
approximate
page_only
unknown
```

This is important because some formats cannot provide exact byte offsets.

---

## 9. Normalization

Normalization should be a distinct stage after extraction.

Recommended operations:

- Unicode normalization;
- newline normalization;
- whitespace normalization;
- optional full-width/half-width normalization;
- case folding for keyword search where appropriate;
- HTML entity decoding;
- removal of non-content script/style blocks.

Normalization must have a version string.

```text
normalization_version = "norm-v1"
```

If normalization behavior changes, affected extraction/chunk/index records may become stale.

---

## 10. localcache Extraction Cache Policy

`localcache` may store extracted segment bundles when this improves development efficiency and runtime performance.

Recommended namespace:

```text
extract-segments:v1
```

Recommended change detection:

```text
MetadataThenFullHash
```

Recommended payload version:

```text
payload_version = EXTRACTED_SEGMENT_BUNDLE_SCHEMA_VERSION
```

Important rules:

1. The cached extraction payload is not authoritative.
2. The authoritative extraction record remains in the `orbok` catalog.
3. Cached extracted text may contain sensitive document content.
4. Privacy-strict mode may disable this cache.
5. Storage view must report this cache under temporary extraction or rebuildable index data.
6. Cleanup must remove it without deleting source files.

## 11. Temporary Extracted Text Policy

`orbok` should not permanently store full extracted text by default.

Allowed behavior:

1. Extract text into memory or temporary working storage.
2. Pass extracted text to chunker and keyword/vector indexing.
3. Store derived indexes and metadata.
4. Delete temporary extracted text after indexing.
5. Optionally cache small snippets according to snippet cache policy.

If an implementation temporarily stores extracted text on disk, it must:

- store it under orbok's cache directory;
- mark it as ephemeral;
- apply cleanup/TTL;
- avoid logs containing its contents;
- not confuse it with persistent catalog data.

---

## 12. Extraction Records

Each extraction attempt creates an extraction record.

Fields:

```text
extraction_id
file_id
extractor_name
extractor_version
normalization_version
source_content_hash
status
extracted_char_count
extracted_byte_count
error_category
error_message
started_at
completed_at
```

Successful extraction does not imply indexing succeeded. Downstream stages must track their own states.

---

## 13. Failure Model

Failure categories:

```text
unsupported_format
permission_denied
file_too_large
encoding_error
parser_error
encrypted_document
file_changed_during_read
timeout
out_of_memory
canceled
internal_error
```

Failure handling:

- record failure per file;
- do not stop whole indexing run;
- expose actionable UI message;
- allow retry where meaningful.

---

## 14. Format-Specific Notes

## 13.1. Plain Text and Logs

- Use streaming read.
- Detect encoding where possible.
- Preserve line numbers.
- Avoid loading huge logs fully into memory.

## 13.2. Markdown

- Preserve heading hierarchy.
- Extract fenced code blocks as meaningful segments.
- Preserve line numbers where possible.

## 13.3. HTML

- Do not execute scripts.
- Remove script/style.
- Treat rendered text extraction as approximate.
- Preserve title and heading hints.
- Do not load remote resources.

## 13.4. PDF

- Extract by page where possible.
- Do not promise exact highlight unless supported.
- Store page number as minimum location.
- Mark location quality as `page_only` or `approximate` when necessary.

## 13.5. DOCX

- Extract paragraphs and headings.
- Store paragraph index where available.
- Treat byte offsets as unavailable.

## 13.6. CSV

- Extract header and rows.
- Preserve row numbers.
- Consider representing each row or row group as segment.

## 13.7. Source Code

- Preserve line numbers.
- Preserve symbols and punctuation.
- Avoid aggressive natural-language normalization for code.

---

## 15. Security Considerations

Document extraction is security-sensitive.

Rules:

- never execute active content;
- treat HTML as untrusted;
- handle parser errors safely;
- avoid unbounded memory allocation;
- enforce file size limits;
- apply cancellation/timeouts;
- redact document text from logs.

---

## 16. UI Requirements

The Indexing view should show extraction failures with clear messages.

Examples:

| Error | User-Facing Message |
|---|---|
| unsupported_format | This file type is not supported yet. |
| encrypted_document | This document appears to be encrypted or password-protected. |
| parser_error | orbok could not extract text from this file. |
| file_too_large | This file exceeds the configured size limit. |
| permission_denied | orbok cannot read this file. Check permissions. |

---

## 17. Acceptance Criteria

- Extractor trait or equivalent abstraction exists.
- Plain text extraction works.
- Markdown extraction works with heading hints.
- HTML extraction removes script/style content.
- PDF extraction records page-level location where available.
- DOCX extraction records paragraph-level approximate location where available.
- CSV extraction records row-level location.
- Source code extraction preserves line numbers.
- Extraction records include extractor and normalization versions.
- Extraction failures are recorded per file.
- Full extracted text is not permanently stored by default.

---

## 18. Testing Requirements

Required tests:

1. Plain text extraction with exact line ranges.
2. Markdown heading extraction.
3. HTML script removal.
4. CSV row extraction.
5. Unsupported file type recorded.
6. Permission denied handled.
7. File too large handled.
8. Parser error handled.
9. Extraction cancellation handled.
10. Extractor version change marks re-extraction requirement.
11. Logs do not contain document body text by default.

PDF and DOCX tests may initially use small fixture files and assert coarse location quality.

---

## 19. Implementation Notes

Recommended Rust modules:

```text
orbok-extract::traits
orbok-extract::text
orbok-extract::markdown
orbok-extract::html
orbok-extract::pdf
orbok-extract::docx
orbok-extract::csv
orbok-extract::code
orbok-extract::normalize
```

Recommended pipeline:

```text
file descriptor
  -> extractor selection
  -> extraction
  -> normalization
  -> extracted segments
  -> temporary buffer
  -> chunking stage
```

---

## 20. Unresolved Questions

- Which PDF extraction crate should be used?
- Which DOCX extraction crate should be used?
- Should HTML boilerplate removal be included in v1?
- Should source code receive language-specific parsing later?
- Should temporary extracted text ever be encrypted on disk?
- Should OCR be a separate future subsystem?

---

## 21. Decision

Implement a versioned, failure-isolated extraction pipeline.

Do not permanently store full extracted text by default.


---

## 22. Amendment: localcache Reference

See `appendices/APPENDIX-A-localcache-integration.md`.

Normative summary:

- extraction may reuse fresh localcache payloads;
- extracted text cache is optional and privacy-sensitive;
- extraction records in the orbok catalog remain authoritative;
- localcache payload version must change when extraction payload schema changes.
