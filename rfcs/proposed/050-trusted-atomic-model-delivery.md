# RFC-050: Trusted Atomic Model Delivery

**Project:** orbok  
**RFC:** 050  
**Title:** Trusted Atomic Model Delivery  
**Status:** Proposed  
**Target milestone:** v1.0.0 security stabilization  
**Date:** 2026-07-14  
**Last revised:** 2026-07-15
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

## 5. Serialized Model-Store Mutation

Every managed model-store mutation for one profile runs under one cross-process
lock rooted at that profile's model directory. The conceptual API is a
`ModelStoreMutationGuard`; implementation may use a reviewed cross-platform
file-lock crate or equivalent OS primitive.

The lock file is `<models-dir>/.model-store.lock`. It is persistent metadata,
but lock ownership is OS-backed and automatically released on process exit. The
worker uses a bounded wait and fails closed on timeout; it never deletes a lock
file to recover ownership.

The exclusive mutation guard spans the complete operation:

- staging creation and partial cleanup;
- generation-directory promotion and post-promotion verification;
- catalog activation or rollback transaction;
- startup recovery classification and mutation;
- generation cleanup and cleanup-eligibility decisions.

All processes and code paths that mutate the managed store must acquire this
same guard. Runtime generation resolution and initial file opening acquire a
shared guard; after the model is fully loaded into memory the shared guard may
be released because generations are immutable. Lock ordering is always model-
store guard first, catalog transaction second. No code may wait for the model-
store guard while holding a catalog transaction.

Cleanup decides eligibility only while holding the exclusive guard. Activation
reads the catalog's current generation inside its commit transaction while the
guard is held and derives `previous` from that value; a readiness report
captured earlier is not authoritative for the pointer switch. Concurrent
install, recovery, rollback, and cleanup operations are therefore serialized
across both filesystem and catalog state.

## 6. Staging and Activation State Machine

For every app-managed installation or repair:

```text
exclusive model-store mutation guard
  -> fresh readiness report
  -> DownloadPlan (skip / download / replace / retry)
  -> create same-filesystem .staging/<install-id>
  -> bounded transfers (maximum 2) into .part files
  -> flush, close, verify exact size and trusted SHA-256 for every file
  -> rename .part files to their staged names
  -> write trusted-manifest.json and COMPLETE
  -> sync the staged tree bottom-up
  -> rename the complete directory to generations/<install-id>
  -> sync both rename parents and model root
  -> validate the immutable generation again
  -> in one transaction read current, then set new current + derived previous
  -> publish Ready only after the catalog transaction commits
```

A valid generation is never changed or deleted during install/repair/update.
Directory rename is same-filesystem and targets a new non-existing name, so it
does not depend on replacing an open file or directory on Windows. The only
activation switch is the SQLite transaction after the new generation is fully
durable and verified. The guard remains held until activation/rollback and
post-commit state checks complete.

## 7. Crash Recovery, Durability, and Lifecycle

Startup recovery runs before model loading:

| Observed state | Recovery |
|---|---|
| incomplete `.staging/<id>` | never ready; remove or quarantine |
| complete generation not referenced by catalog | validate, then retain as inactive or remove; never auto-activate |
| crash before catalog commit | SQLite keeps old current generation; new complete generation remains inactive |
| crash during catalog commit | SQLite atomicity yields either old or new current/previous pair |
| current `B`, previous `A`; `B` missing/invalid and `A` verifies | mark `B` invalid and atomically set `(current=A, previous=NULL)` |
| current and previous both missing/invalid | mark invalid records, atomically set `(current=NULL, previous=NULL)`, and do not report Ready |
| cleanup interrupted | active and previous generations remain; extra inactive data is safe |

### 7.1. Exact Sync Order

All paths are on the model store's filesystem. Where the platform supports
directory syncing, the required order is:

1. Create `.staging/`, `generations/`, and the unique staging tree; sync newly
   modified parent directories before transfer begins.
2. After download and trusted verification, call `sync_all` on every `.part`
   file before renaming it to its staged final name.
3. After file renames, sync each modified nested directory bottom-up: `onnx/`
   first, then the staging generation root.
4. Write and `sync_all` `trusted-manifest.json`, then `COMPLETE`; sync the
   staging generation root again.
5. Rename `.staging/<id>` to `generations/<id>` using a new non-existing target.
6. Sync the source parent `.staging/`, then destination parent `generations/`,
   then the model root.
7. Re-verify the promoted generation before opening the catalog transaction.

A platform helper encapsulates directory sync semantics and is tested on Unix
and Windows. If a target cannot provide the required rename/durability contract,
installation fails before activation. The design does not promise that the
latest activation survives power loss beyond the catalog's existing WAL plus
`synchronous=NORMAL` policy; the safe permitted outcome is the prior coherent
catalog pointer and an inactive complete generation.

### 7.2. Durable Startup Validation

The catalog stores a monotonically increasing per-profile `startup_epoch` and,
for each managed generation, its `activation_epoch` plus optional
`validated_startup_epoch`.

- Activation records `activation_epoch = current_startup_epoch` and clears
  `validated_startup_epoch` for the new current generation.
- A later process startup increments and durably records `startup_epoch` under
  the exclusive guard before recovery.
- The new current becomes startup-validated only when
  `current_startup_epoch > activation_epoch` and that startup completes trusted
  digest verification plus tokenizer/ONNX load and dimension checks.
- Only then is `validated_startup_epoch` recorded.
- A second activation is rejected while the current generation lacks this
  later-startup validation. An initial activation from `current=NULL` is not a
  second activation and does not require a predecessor validation record.
- Current and previous ids are never cleanup-eligible. A previous generation
  becomes ordinary inactive data only after the current generation has a
  recorded later-startup validation. Cleanup rechecks all conditions under the
  exclusive guard.

This durable epoch rule prevents a second activation in the original process
from discarding the last generation known to survive a real restart.

### 7.3. Rollback Pair Semantics

Rollback validates the recorded previous generation while holding the exclusive
guard. For `(current=B, previous=A)`, verified `A`, and invalid `B`, one catalog
transaction marks `B` invalid and writes `(current=A, previous=NULL)`. Invalid
`B` is never retained as a rollback target; current and previous may not be
equal. If `A` also fails verification, the transaction writes
`(current=NULL, previous=NULL)` and the model is not Ready.

If the platform cannot provide the required rename/durability behavior, the
worker fails closed before activation. No `.bak` replacement protocol is used.

## 8. Network and Source Policy

- Downloads require an explicit user action and display model identity, source,
  approximate size, license, storage location, verification status, and the
  local-only privacy statement.
- Redirect behavior, permitted hosts, header stripping, and credential policy
  are exactly those in Appendix B.
- The production client disables automatic environment/system proxy discovery
  and configures no proxy, credential, cookie store, or automatic referer.
- HTTP is forbidden.
- No credentials, document content, queries, source paths, or local model paths
  are sent with model requests.
- Logs contain model/file logical identifiers and safe error classes, not URL
  query strings or local paths under strict privacy.
- Timeouts, size limits, and bounded concurrency are mandatory.

## 9. Parser Threat Boundary

Checksum verification establishes that bytes match the reviewed artifact; it
does not make model formats intrinsically safe. The threat model must record
ONNX and tokenizer parsing as untrusted-input processing and preserve:

- parser/library patch discipline;
- file-size and tensor/dimension limits before inference;
- typed failure without marking the model ready;
- no document upload or remote validation.

Sandboxing model inference is not required by this RFC, but any remaining
parser risk must be stated in release security documentation.

## 10. Failure and Recovery Rules

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
- Mutation timeout or lock failure leaves the store unchanged and does not fall
  back to an unlocked path.

## 11. Non-Goals

- Silent model download or update.
- Arbitrary user-provided download URLs.
- Selecting a different default model.
- Remote document/query validation.
- A general package-signing framework beyond the reviewed embedded manifest.

## 12. Testing Requirements

Tests must cover skip, fresh download, invalid replacement, interrupted retry,
checksum mismatch, size overflow, redirect rejection, network failure, atomic
promotion, bounded concurrency, and every crash state in §7. Tests must prove
the old generation remains active before commit, the complete new generation
becomes active after commit, invalid current state rolls back only to a verified
previous generation, and no mixed generation can be loaded. Windows tests must
exercise directory promotion while the old generation is open and recovery
after each injected activation crash point. At least one end-to-end app-layer
test must exercise the same worker invoked by the GUI against a local mock
server.

Adversarial concurrency tests must interleave installer, recovery, rollback,
and cleanup attempts from separate processes. They must prove exclusive mutation
ownership, shared-reader/exclusive-writer behavior, cleanup cannot remove a
promoted pre-activation generation, activation derives `previous` at commit
time, lock timeout fails closed, and a crashed owner releases the OS lock.

Lifecycle tests must cover exact rollback pairs, startup-epoch persistence,
rejection of a second activation before later-startup validation, eligibility
only after digest plus real backend-load validation on a later startup, and
cleanup protection for current/previous generations. Proxy tests set
credential-bearing `HTTP_PROXY`, `HTTPS_PROXY`, and `ALL_PROXY` values and prove
the production client neither connects to the proxy nor emits proxy
authorization.

## 13. Acceptance Criteria

This RFC is accepted when Appendix B passes independent security/design review,
and the serialized immutable-generation activation, durability, and
crash-recovery protocol are approved.

It is implemented when the GUI uses the readiness/plan worker, app-managed
files verify against reviewed metadata before generation activation, failures
preserve the last coherent generation, every recoverable disk state is handled,
user-visible trust status is localized, the threat model is updated, and the
cross-platform end-to-end failure/recovery suite passes.
