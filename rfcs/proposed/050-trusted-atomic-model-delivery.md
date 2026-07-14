# RFC-050: Trusted Atomic Model Delivery

**Project:** orbok  
**RFC:** 050  
**Title:** Trusted Atomic Model Delivery  
**Status:** Proposed  
**Target milestone:** v1.0.0 security stabilization  
**Date:** 2026-07-14  
**Related RFCs:** RFC-012 Model Registry; RFC-021 Default Embedding Model; RFC-029 Model Download Integrity and Trust; RFC-043 Model Download Readiness  
**Handoff:** [`HANDOFF-050-trusted-atomic-model-delivery.md`](../handoffs/HANDOFF-050-trusted-atomic-model-delivery.md)

**Trust root:** [`APPENDIX-B-default-model-trust-root.md`](../appendices/APPENDIX-B-default-model-trust-root.md)

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
`intfloat/multilingual-e5-small`, selected by RFC-021. The exact full revision,
URLs, SHA-256 digests, sizes, transfer limits, license, model identity, and host
policy are normative in Appendix B. That reviewed artifact, embedded into the
application, is the root of trust; production code does not fetch or invent it.

For each required file the manifest records:

- logical name and final relative path;
- immutable revision-qualified HTTPS source;
- expected SHA-256 digest;
- expected size when stable and available;
- model id, version/revision, role, dimension, and license summary.

The trusted manifest is reviewed source material. Downloaded metadata,
redirect responses, or a manifest stored beside downloaded files cannot replace
it as the root of trust. Updating a trusted digest or source revision is a
reviewed repository change.

Manual/offline model placement remains supported, but the UI must distinguish
`App verified` from `User supplied / provenance not verified` rather than
claiming equivalent trust.

## 4. Generation Transaction Boundary

Atomicity is per complete manifest generation, not per file. A generation is a
unique immutable directory containing every required file, the trusted manifest
identity, and a completion marker. A file is never promoted into the currently
active generation.

Managed layout:

```text
models/multilingual-e5-small/
  .staging/<install-id>/...
  generations/<install-id>/
    tokenizer.json
    onnx/model.onnx
    trusted-manifest.json
    COMPLETE
```

`<install-id>` is unique even when reinstalling the same revision. The catalog
stores the current and previous active generation ids in one SQLite transaction.
Runtime model loading resolves only the catalog's current generation. Manual
user-supplied folders remain outside this managed generation store and are never
modified by the downloader.

## 5. Staging and Activation State Machine

For every app-managed installation or repair:

```text
fresh readiness report
  -> DownloadPlan (skip / download / replace / retry)
  -> create same-filesystem .staging/<install-id>
  -> bounded transfers (maximum 2) into .part files
  -> flush, close, verify exact size and trusted SHA-256 for every file
  -> rename .part files to their staged names
  -> write trusted-manifest.json and COMPLETE
  -> flush files and staging directory
  -> rename the complete directory to generations/<install-id>
  -> flush the generations parent where supported
  -> validate the immutable generation again
  -> atomically update catalog current + previous generation ids
  -> publish Ready only after the catalog transaction commits
```

A valid generation is never changed or deleted during install/repair/update.
Directory rename is same-filesystem and targets a new non-existing name, so it
does not depend on replacing an open file or directory on Windows. The only
activation switch is the SQLite transaction after the new generation is fully
durable and verified.

## 6. Crash Recovery and Durability

Startup recovery runs before model loading:

| Observed state | Recovery |
|---|---|
| incomplete `.staging/<id>` | never ready; remove or quarantine |
| complete generation not referenced by catalog | validate, then retain as inactive or remove; never auto-activate |
| crash before catalog commit | SQLite keeps old current generation; new complete generation remains inactive |
| crash during catalog commit | SQLite atomicity yields either old or new current/previous pair |
| current generation missing or invalid | do not report Ready; atomically roll back to the recorded previous generation only after it verifies |
| cleanup interrupted | active and previous generations remain; extra inactive data is safe |

Every downloaded file is `sync_all`'d after verification. The manifest and
completion marker are also flushed. Directory metadata is synced where the
platform exposes a supported operation. The catalog activation transaction uses
the project's durable SQLite policy. The previous verified generation is
retained through at least one successful subsequent startup; cleanup never
removes current or rollback generations.

If the platform cannot provide the required rename/durability behavior, the
worker fails closed before activation. No `.bak` replacement protocol is used.

## 7. Network and Source Policy

- Downloads require an explicit user action and display model identity, source,
  approximate size, license, storage location, verification status, and the
  local-only privacy statement.
- Redirect behavior, permitted hosts, header stripping, and credential policy
  are exactly those in Appendix B.
- HTTP is forbidden.
- No credentials, document content, queries, source paths, or local model paths
  are sent with model requests.
- Logs contain model/file logical identifiers and safe error classes, not URL
  query strings or local paths under strict privacy.
- Timeouts, size limits, and bounded concurrency are mandatory.

## 8. Parser Threat Boundary

Checksum verification establishes that bytes match the reviewed artifact; it
does not make model formats intrinsically safe. The threat model must record
ONNX and tokenizer parsing as untrusted-input processing and preserve:

- parser/library patch discipline;
- file-size and tensor/dimension limits before inference;
- typed failure without marking the model ready;
- no document upload or remote validation.

Sandboxing model inference is not required by this RFC, but any remaining
parser risk must be stated in release security documentation.

## 9. Failure and Recovery Rules

- Interrupted `.part` files are never treated as ready.
- Retry may discard and restart a partial file unless safe resumable semantics
  are separately designed.
- Digest, size, redirect-policy, filesystem, or parser validation failure keeps
  the prior coherent generation active and presents a localized recoverable
  error.
- A process restart begins with a new readiness report and plan.
- The application must not synthesize new expected digests from received bytes.
- No recoverable failure may destroy the last coherent verified generation.
- Mixed-revision file sets are impossible because activation points to one
  immutable generation directory.

## 10. Non-Goals

- Silent model download or update.
- Arbitrary user-provided download URLs.
- Selecting a different default model.
- Remote document/query validation.
- A general package-signing framework beyond the reviewed embedded manifest.

## 11. Testing Requirements

Tests must cover skip, fresh download, invalid replacement, interrupted retry,
checksum mismatch, size overflow, redirect rejection, network failure, atomic
promotion, bounded concurrency, and every crash state in §6. Tests must prove
the old generation remains active before commit, the complete new generation
becomes active after commit, invalid current state rolls back only to a verified
previous generation, and no mixed generation can be loaded. Windows tests must
exercise directory promotion while the old generation is open and recovery
after each injected activation crash point. At least one end-to-end app-layer
test must exercise the same worker invoked by the GUI against a local mock
server.

## 12. Acceptance Criteria

This RFC is accepted when Appendix B passes independent security/design review,
and the immutable-generation activation and crash-recovery protocol are
approved.

It is implemented when the GUI uses the readiness/plan worker, app-managed
files verify against reviewed metadata before generation activation, failures
preserve the last coherent generation, every recoverable disk state is handled,
user-visible trust status is localized, the threat model is updated, and the
cross-platform end-to-end failure/recovery suite passes.
