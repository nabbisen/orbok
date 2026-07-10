# RFC-029: Model Download Integrity and Trust Policy

**Project:** orbok  
**RFC:** 029  
**Title:** Model Download Integrity and Trust Policy  
**Status:** Implemented (v0.7.0)
**Target Timing:** Before enabling automatic or semi-automatic model downloads  
**Date:** 2026-06-06

> **Future RFC Notice:** This RFC is intentionally deferred. It must be reconsidered only after the basic implementation tasks from RFC-001 through RFC-020 are substantially complete, tested, and benchmarked. It must not block the initial implementation of `orbok`.

---


## 1. Summary

This future RFC will define integrity and trust policy for downloading local AI model files.

The current model workflow may allow explicit installation later, but automatic or semi-automatic downloads must not be implemented without a trust policy.

## 2. Motivation

Model files are large and may come from external sources. Risks include corrupted downloads, malicious model files, license confusion, unexpected network access, user privacy misunderstandings, supply-chain compromise, and incompatible model versions.

`orbok` must not silently download models.

## 3. Activation Conditions

Reconsider this RFC when:

1. Model registry exists.
2. Model installation UI exists or is planned.
3. Default model candidates are known.
4. Distribution source is selected.
5. Checksum/signature options are known.

## 4. Trust Requirements

Model installation must show:

- model name;
- source URL or provider;
- file size;
- license summary;
- role: embedding/reranker;
- checksum or signature status;
- storage path;
- privacy statement.

User-facing statement:

```text
Installing a model downloads model files only. Your documents are not uploaded.
```

## 5. Integrity Options

| Option | Strength |
|---|---|
| HTTPS only | weak baseline |
| SHA256 checksum | basic integrity |
| signed checksum file | stronger |
| repository tag/release verification | stronger |
| embedded trusted manifest | good app-managed path |

## 6. Model Manifest

Potential manifest:

```json
{
  "model_id": "embedding-model-v1",
  "role": "embedding",
  "name": "...",
  "version": "...",
  "files": [
    {
      "url": "...",
      "sha256": "...",
      "size_bytes": 123
    }
  ],
  "license": "...",
  "dimension": 768,
  "backend": "candle"
}
```

## 7. Non-Goals

This future RFC should not select the default embedding model, enable silent downloads, support arbitrary unverified model URLs by default, or upload documents to validate models.

## 8. Expected Decision Output

The activated RFC should produce trusted model source policy, manifest format, checksum/signature policy, download UX, offline install workflow, license display, failure handling, and cache/storage policy.

## 9. Acceptance Criteria

- Model downloads require explicit user confirmation.
- Checksum or stronger integrity check defined.
- License summary shown.
- Offline/manual model placement supported.
- Documents are not uploaded.
- Failed download is recoverable.
- Model path validation exists.

## 10. Deferred Decision

Do not implement automatic model downloads until this trust policy is accepted.
