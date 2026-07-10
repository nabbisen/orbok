# RFC-025: OCR Pipeline

**Project:** orbok  
**RFC:** 025  
**Title:** OCR Pipeline  
**Status:** Implemented (v0.8.0)
**Target Timing:** After text-based document search is stable and scanned-document demand is confirmed  
**Date:** 2026-06-06

> **Future RFC Notice:** This RFC is intentionally deferred. It must be reconsidered only after the basic implementation tasks from RFC-001 through RFC-020 are substantially complete, tested, and benchmarked. It must not block the initial implementation of `orbok`.

---


## 1. Summary

This future RFC will define OCR support for image files and scanned PDFs.

OCR is valuable, but it is expensive, error-prone, and substantially expands the scope of `orbok`. It should not block the initial local text document search implementation.

## 2. Motivation

OCR support would enable search over scanned PDFs, screenshots, photos of documents, image-based reports, and printed notes. It also adds heavy model/runtime dependencies, language-specific accuracy challenges, high CPU/GPU cost, more storage, privacy sensitivity, and complex UI expectations.

## 3. Activation Conditions

Reconsider this RFC when:

1. Baseline text extraction is stable.
2. PDF extraction backend is selected.
3. Benchmark and storage dashboards exist.
4. User/product need for scanned documents is confirmed.
5. Model/runtime options are evaluated.

## 4. Scope Candidates

| Level | Description |
|---|---|
| L0 | No OCR, detect scanned PDFs only |
| L1 | OCR scanned PDFs manually on demand |
| L2 | OCR images and scanned PDFs in selected sources |
| L3 | Background OCR with queue/resource controls |
| L4 | Layout-aware OCR and table extraction |

Recommended first OCR scope should be L0 or L1.

## 5. Evaluation Criteria

Evaluate OCR accuracy for English and Japanese, runtime performance, model size, CPU/GPU dependency, layout preservation, confidence scores, license, integration complexity, and storage impact.

## 6. Data Lifecycle

OCR output may contain sensitive extracted text.

Classify OCR outputs as rebuildable index data or ephemeral cache data if temporary. OCR text should not be permanently stored as full extracted text by default unless explicitly configured.

## 7. UI Requirements

If OCR is unsupported:

```text
This PDF appears to contain scanned images. Text search may not work until OCR support is enabled.
```

If OCR is optional:

```text
Run local OCR for this file?
This may take time and will store derived searchable text locally.
```

## 8. Non-Goals

This future RFC should not block text extraction, require cloud OCR, silently OCR every image, guarantee handwriting recognition, or OCR sensitive folders without user awareness.

## 9. Expected Decision Output

The activated RFC should produce OCR scope level, OCR engine/model, language support, storage policy, resource controls, UI workflow, benchmark result, and privacy notes.

## 10. Acceptance Criteria

- Scanned-document detection exists before full OCR.
- OCR engine evaluated.
- Japanese OCR quality tested if claimed.
- Storage impact measured.
- User consent workflow defined.
- OCR output lifecycle defined.
- Cleanup behavior defined.

## 11. Deferred Decision

OCR is deferred until ordinary text-based search is stable.
