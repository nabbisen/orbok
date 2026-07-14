# RFC-050: Trusted Atomic Model Delivery

**Project:** orbok  
**RFC:** 050  
**Title:** Trusted Atomic Model Delivery  
**Status:** Proposed  
**Target milestone:** v1.0.0 security stabilization  
**Date:** 2026-07-14  
**Related RFCs:** RFC-012 Model Registry; RFC-021 Default Embedding Model; RFC-029 Model Download Integrity and Trust; RFC-043 Model Download Readiness  
**Handoff:** [`HANDOFF-050-trusted-atomic-model-delivery.md`](../handoffs/HANDOFF-050-trusted-atomic-model-delivery.md)

---

## 1. Summary

This RFC activates and completes the trust and atomicity decisions that the
implemented RFC-029 and RFC-043 paths do not currently enforce end to end.

App-managed model downloads must be derived from a fresh readiness report,
downloaded to temporary files, authenticated against application-trusted
metadata, and atomically promoted. Hashing received bytes and then writing
those hashes beside the same bytes is not provenance verification.

## 2. Triggering Evidence

The architecture preparation review found that the GUI downloader:

- bypasses the existing `DownloadPlan` contract;
- writes directly to final paths and removes them on failure;
- does not implement bounded plan execution or atomic promotion;
- creates a manifest from newly downloaded bytes, which detects later changes
  but cannot establish original provenance.

These are v1.0.0-blocking reliability and supply-chain findings.

## 3. Trust Decision

The app-managed default model remains
`intfloat/multilingual-e5-small`, selected by RFC-021. It is identified at a
full immutable Hugging Face commit revision (never `main`) by an
application-trusted manifest distributed with orbok. For each required file the
manifest records:

- logical name and final relative path;
- immutable revision-qualified HTTPS source;
- expected SHA-256 digest;
- expected size when stable and available;
- model id, version/revision, role, dimension, and license summary.

The trusted manifest must be reviewed source material. Downloaded metadata,
redirect responses, or a manifest stored beside downloaded files cannot replace
it as the root of trust. Updating a trusted digest or source revision is a
reviewed repository change.

Manual/offline model placement remains supported, but the UI must distinguish
`App verified` from `User supplied / provenance not verified` rather than
claiming equivalent trust.

## 4. Atomic Delivery State Machine

For every app-managed installation or repair:

```text
fresh readiness report
  -> DownloadPlan (skip / download / replace / retry)
  -> bounded transfers (maximum 2)
  -> write only to .part paths
  -> flush and close
  -> verify size and trusted SHA-256
  -> atomically rename into the final path
  -> re-run readiness and deep verification
  -> publish Ready only if every required file passes
```

A valid final file is never truncated or deleted before its replacement has
passed verification. Where replacement cannot be an atomic rename on a target
platform, implementation must use a same-filesystem backup/rollback protocol
and test the failure path.

## 5. Network and Source Policy

- Downloads require an explicit user action and display model identity, source,
  approximate size, license, storage location, verification status, and the
  local-only privacy statement.
- Redirects are limited. Every permitted artifact host is recorded in the
  reviewed trusted-manifest/source policy; any other redirect is rejected.
- HTTP is forbidden.
- No credentials, document content, queries, source paths, or local model paths
  are sent with model requests.
- Logs contain model/file logical identifiers and safe error classes, not URL
  query strings or local paths under strict privacy.
- Timeouts, size limits, and bounded concurrency are mandatory.

## 6. Parser Threat Boundary

Checksum verification establishes that bytes match the reviewed artifact; it
does not make model formats intrinsically safe. The threat model must record
ONNX and tokenizer parsing as untrusted-input processing and preserve:

- parser/library patch discipline;
- file-size and tensor/dimension limits before inference;
- typed failure without marking the model ready;
- no document upload or remote validation.

Sandboxing model inference is not required by this RFC, but any remaining
parser risk must be stated in release security documentation.

## 7. Failure and Recovery Rules

- Interrupted `.part` files are never treated as ready.
- Retry may discard and restart a partial file unless safe resumable semantics
  are separately designed.
- Digest, size, redirect-policy, filesystem, or parser validation failure keeps
  the prior valid final file and presents a localized recoverable error.
- A process restart begins with a new readiness report and plan.
- The application must not synthesize new expected digests from received bytes.

## 8. Non-Goals

- Silent model download or update.
- Arbitrary user-provided download URLs.
- Selecting a different default model.
- Remote document/query validation.
- A general package-signing framework beyond the reviewed embedded manifest.

## 9. Testing Requirements

Tests must cover skip, fresh download, invalid replacement, interrupted retry,
checksum mismatch, size overflow, redirect rejection, network failure, atomic
promotion, rollback/preservation of a valid model, bounded concurrency, restart
recovery, and final readiness recheck. At least one end-to-end app-layer test
must exercise the same worker invoked by the GUI against a local mock server.

## 10. Acceptance Criteria

This RFC is accepted when the immutable-revision manifest policy,
manual-model trust vocabulary, and atomic replacement behavior are approved.

It is implemented when the GUI uses the readiness/plan worker, app-managed
files verify against reviewed metadata before promotion, failures preserve
valid files, user-visible trust status is localized, the threat model is
updated, and the end-to-end failure/recovery suite passes.
