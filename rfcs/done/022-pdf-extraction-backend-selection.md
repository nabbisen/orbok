# RFC-022: PDF Extraction Backend Selection

**Project:** orbok  
**RFC:** 022  
**Title:** PDF Extraction Backend Selection  
**Status:** Implemented (v0.7.0)
**Target Timing:** After baseline extraction pipeline and PDF fixtures exist  
**Date:** 2026-06-06

> **Future RFC Notice:** This RFC is intentionally deferred. It must be reconsidered only after the basic implementation tasks from RFC-001 through RFC-020 are substantially complete, tested, and benchmarked. It must not block the initial implementation of `orbok`.

---


## 1. Summary

This future RFC will select the production PDF extraction backend for `orbok`.

PDF extraction is difficult enough to deserve a dedicated decision after baseline document extraction is implemented and benchmark fixtures exist.

## 2. Motivation

PDFs are important for document search, but they are risky because text order may be unreliable, scanned PDFs contain no text, page/region mapping may be approximate, malformed PDFs may crash parsers, and extraction crates vary in licensing, quality, and platform support.

## 3. Activation Conditions

Reconsider this RFC when:

1. RFC-005 extraction pipeline exists.
2. RFC-006 location quality model exists.
3. PDF fixture corpus exists.
4. Search result preview can show page-level context.
5. Parser failure isolation is implemented.
6. Benchmark tools can compare extraction outputs.

## 4. Candidate Evaluation Criteria

| Criterion | Why It Matters |
|---|---|
| Text extraction quality | Search recall |
| Reading order quality | Snippet usefulness |
| Page mapping | Preview and location labels |
| Error isolation | Robust indexing |
| Memory behavior | Large PDFs |
| License | Redistribution |
| Platform support | Linux/Windows/macOS |
| Rust integration | Maintenance and safety |
| Security posture | Parser risk |

## 5. Required Test Fixtures

Prepare fixtures for simple text PDF, multi-column PDF, Japanese PDF, scanned image-only PDF, encrypted PDF, malformed/corrupt PDF, long PDF, PDF with tables, and PDF with headers/footers.

## 6. Expected Decision Output

The activated RFC should produce:

```text
selected PDF backend
fallback backend if any
supported PDF feature level
location quality guarantees
known limitations
failure categories
security notes
test results
```

## 7. Non-Goals

This future RFC should not implement OCR, guarantee pixel-perfect highlighting, support active PDF content, require encrypted PDFs, or block basic text/Markdown indexing.

## 8. Acceptance Criteria

- At least two PDF extraction approaches evaluated if practical.
- Fixture corpus results documented.
- Japanese PDF extraction assessed.
- Failure isolation tested.
- Location quality claims are honest.
- Security concerns documented.

## 9. Deferred Decision

No PDF backend is selected by this future RFC draft.

Baseline extraction may use a simple temporary backend until this RFC is activated.
