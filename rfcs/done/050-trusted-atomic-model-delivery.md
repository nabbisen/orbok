# RFC-050: Trusted Atomic Model Delivery

**Project:** orbok  
**RFC:** 050  
**Title:** Trusted Atomic Model Delivery  
**Status:** Implemented (main at `902f33a`; release pending)
**Target milestone:** v1.0.0 security stabilization  
**Date:** 2026-07-14  
**Last revised:** 2026-07-16
**Related RFCs:** RFC-012 Model Registry; RFC-021 Default Embedding Model; RFC-029 Model Download Integrity and Trust; RFC-043 Model Download Readiness  
**Handoff:** [`HANDOFF-050-trusted-atomic-model-delivery.md`](../handoffs/HANDOFF-050-trusted-atomic-model-delivery.md)

**Trust root:** [`APPENDIX-B-default-model-trust-root.md`](../appendices/APPENDIX-B-default-model-trust-root.md)
**Phase 4 consent/threat delta:** [`APPENDIX-C-rfc050-phase4-consent-threat-model.md`](../appendices/APPENDIX-C-rfc050-phase4-consent-threat-model.md)
**Phase 4 GUI lifecycle design:** [`APPENDIX-D-rfc050-gui-lifecycle-integration-design.md`](../appendices/APPENDIX-D-rfc050-gui-lifecycle-integration-design.md)
**Phase 4 compositional proof:** [`APPENDIX-E-rfc050-phase4-compositional-proof-report.md`](../appendices/APPENDIX-E-rfc050-phase4-compositional-proof-report.md)

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
  -> rename .part files and complete each platform durability barrier
  -> write trusted-manifest.json and COMPLETE
  -> complete the platform namespace barriers
  -> rename the complete directory to generations/<install-id>
  -> complete the platform promotion barriers
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

### 7.1. Platform Namespace Durability Protocol

All regular payload, manifest, and completion-marker files use the same content
protocol on every platform:

1. Write a `.part` or metadata file.
2. Flush and call file `sync_all` before closing it.
3. Verify the trusted exact size and digest where applicable.
4. Complete the platform rename barrier before treating a staged final name as
   durable.

`COMPLETE` cannot participate in promotion until every payload and metadata
file has completed its regular-file sync. Namespace durability is then
platform-specific; Windows directory `File::sync_all` is not part of this
contract.

#### 7.1.1. Unix barriers

Unix uses same-filesystem rename plus bottom-up directory `sync_all`:

| Order | Barrier | Crash-safe disposition |
|---|---|---|
| 1 | Create `.staging/`, `generations/`, and the unique staging tree; sync modified parents | Missing or incomplete staging is never loadable |
| 2 | Before/after each `.part` rename | Incomplete staging is quarantined or removed |
| 3 | Sync modified nested directories, deepest first, then the staging generation root | Only fully named, file-synced payloads advance |
| 4 | Write/sync `trusted-manifest.json`, then `COMPLETE`; sync the staging root | An unsynced/incomplete generation cannot promote |
| 5 | Before/after rename from `.staging/<id>` to new `generations/<id>` | State is staging or a complete unreferenced generation |
| 6 | Sync `.staging/`, then `generations/`, then the model root | Promotion namespace changes precede catalog mutation |
| 7 | Re-verify the immutable generation | Failure prevents registration/activation |

Recovery and quarantine renames sync their affected parents in the same
source-parent, destination-parent, model-root order. Cleanup deletion is
rechecked under the exclusive guard; interruption may over-retain inactive or
invalid bytes but cannot remove current or previous generations.

#### 7.1.2. Windows barriers

Windows uses one shared wide-character `durable_rename` primitive implemented
with `MoveFileExW` and exactly `MOVEFILE_WRITE_THROUGH`. It must not pass
`MOVEFILE_REPLACE_EXISTING`, `MOVEFILE_COPY_ALLOWED`,
`MOVEFILE_DELAY_UNTIL_REBOOT`, or any other move flag. Source and destination
must be on the same supported local volume and the destination must not exist.
A false return is a typed filesystem failure and prevents any catalog mutation
that assumes the move succeeded.

The primitive is mandatory for:

- every `.part` to staged-final payload rename;
- `.staging/<id>` to `generations/<id>` promotion;
- a recovery move from staging to a complete unreferenced generation;
- quarantine moves from `.staging` or `generations`.

| Order | Barrier | Crash-safe disposition |
|---|---|---|
| 1 | Create staging parents/tree; no unsupported directory flush is claimed | Missing or incomplete staging is never loadable |
| 2 | Before/after each write-through staged-file rename | A `.part` or staged final file remains non-loadable without the complete set |
| 3 | Write and file-sync manifest and `COMPLETE` | File contents are durable before promotion |
| 4 | Before/after write-through generation promotion | State is incomplete staging or a complete unreferenced generation |
| 5 | Re-verify the immutable promoted generation | Failure prevents registration/activation |

Before/after boundaries also wrap each write-through recovery/quarantine move.
An interrupted creation is absent or incomplete staging. An interrupted cleanup
deletion may leave extra inactive/invalid bytes and is retried. Neither case may
modify or delete current/previous generations. A successful write-through
promotion followed by a pre-catalog crash is recovered only after complete
validation and only as inactive data; it is never auto-activated.

#### 7.1.3. Shared catalog and lifecycle barriers

After the platform namespace barriers and immutable-generation revalidation,
Unix and Windows use the same SQLite crash boundaries:

| Order | Before/after transaction | Crash-safe disposition |
|---|---|---|
| 1 | Inactive registration | Before commit, complete generation data is unreferenced; after commit, the generation is exactly `Inactive` |
| 2 | Activation | Before commit, the prior pointer pair remains and the candidate is inactive; after commit, the candidate is current and commit-time current is previous |
| 3 | Invalid-current rollback with verified previous `A` | Before commit, coherent `(current=B, previous=A)` remains and startup retries; after commit, state is exactly `(current=A, previous=NULL)` and `B` is invalid |
| 4 | Invalid-current rollback with invalid/unverifiable previous `A` | Before commit, coherent `(current=B, previous=A)` remains and startup retries; after commit, state is exactly `(current=NULL, previous=NULL)` and both failed records are invalid |
| 5 | Later-startup validation | Before commit, current remains unvalidated and previous stays protected; after commit, validation is durable and previous remains protected until release |
| 6 | Predecessor release | Before commit, previous is safely over-retained; after commit, `previous=NULL` and the former previous record is inactive |

Every row requires abrupt-exit injection immediately before and after commit on
Windows and Unix-like targets. Rollback may never leave invalid `B` as previous,
retain an invalid rollback target, or produce equal current and previous ids.
SQLite atomicity permits only the complete pre-commit or post-commit state for
the selected branch.

#### 7.1.4. Windows path and volume contract

The Windows helper must preserve the effective absolute and extended-length
path behavior provided by Rust filesystem APIs:

- use `MoveFileExW` with non-lossy UTF-16 conversion and reject interior NUL;
- require absolute managed-store paths and reject relative, drive-relative, and
  root-relative inputs;
- preserve already-verbatim paths; convert drive-absolute paths to `\\?\C:\...`
  form and UNC paths to `\\?\UNC\server\share\...` without resolving through a
  reparse point;
- preserve roots, Unicode components, and non-existing destination leaves;
- validate the managed root and every existing ancestor without following an
  unreviewed reparse boundary; derive a non-existing destination's volume from
  an existing validated parent or opened managed-root handle, not lexical drive
  letters alone;
- reject malformed prefixes and propagate the raw Win32 error into safe test
  diagnostics while retaining the public typed filesystem error.

Initial Windows support is limited to local fixed NTFS or ReFS volumes. Source
and destination volume identity and filesystem type are checked before the
move. Installation preflights the managed-store path and volume before staging
or network transfer. UNC/network shares, redirected network application-data
roots, removable media, FAT/exFAT, cross-volume moves, and unknown
filesystem/drive types fail closed before activation. Supporting them requires
separate durability evidence and design review.

The design does not promise that the latest activation survives power loss
beyond the catalog's existing WAL plus `synchronous=NORMAL` policy. The safe
permitted outcome is the prior coherent catalog pointer or a complete
unreferenced/inactive generation.

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
after each injected activation crash point.

The immutable production entry cannot be redirected to a local fixture without
weakening its reviewed trust and transport binding. GUI-to-worker acceptance
therefore uses the named compositional proof defined by Appendix D rather than
claiming one app-layer end-to-end local-mock test. That proof must cover the
compiled app controller and adapter, the private production transaction core
against a local mock server, and the production entry's wrapper obligations
separately. It must also prove the direct production binding from the GUI
adapter to `install_default_model`. No component test may be described as the
literal production worker running against localhost, and no runtime or release
configuration may select test transport, metadata, or trust roots.

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

Windows helper tests must cover write-through file and directory moves,
nonexistent-destination enforcement, same-volume enforcement, unsupported
volume/filesystem failure, Unicode, extended-length drive paths, UNC conversion
followed by policy rejection, interior-NUL rejection, and raw OS error
diagnostics. The crash matrix must use the Windows barriers in §7.1.2 rather
than obsolete Unix directory-sync points, followed by every shared catalog
barrier in §7.1.3.

The Windows reparse-boundary test must create a real directory junction or
equivalent `FILE_ATTRIBUTE_REPARSE_POINT` fixture without requiring elevated
symlink privilege, assert that the fixture is a reparse point, and then prove
production validation rejects it. OS error 1314 is not a passing skip.
Preflight tests must also reject a lexically local managed path whose existing
ancestor is a junction/reparse point into unsupported storage.

## 13. Acceptance Criteria

This RFC is accepted when Appendix B passes independent security/design review,
and the serialized immutable-generation activation, durability, and
crash-recovery protocol are approved.

It is implemented when the GUI uses the readiness/plan worker, app-managed
files verify against reviewed metadata before generation activation, failures
preserve the last coherent generation, every recoverable disk state is handled,
user-visible trust status is localized, the threat model is updated, and the
cross-platform end-to-end failure/recovery suite passes.
