# Implementation Handoff — RFC-050: Trusted Atomic Model Delivery

**Project:** orbok  
**RFC:** 050  
**Lifecycle stage:** Design + handoff  
**Primary owners:** model registry/readiness, app download worker, security  
**Last revised:** 2026-07-16
**RFC:** [`../proposed/050-trusted-atomic-model-delivery.md`](../proposed/050-trusted-atomic-model-delivery.md)

**Trust root:** [`../appendices/APPENDIX-B-default-model-trust-root.md`](../appendices/APPENDIX-B-default-model-trust-root.md)

> **Scope rule:** Preserve valid model files until a trusted replacement has
> passed verification. Never derive expected integrity metadata from the bytes
> being accepted.

## 1. Approval Gate

Do not implement production network or activation code until Appendix B and
RFC-050's generation protocol receive independent security/design approval.
The pure manifest/policy subset in §3A is already approved. Generation schema,
filesystem, catalog mutation, production HTTP, recovery, cleanup, and GUI work
remain blocked until the revised serialization/lifecycle protocol is reviewed.

## 2. Expected Change Surface

- `crates/search/models/src/readiness.rs`
- `crates/search/models/src/download_plan.rs`
- a reviewed trusted-manifest module/asset in `orbok-models` or `orbok-app`
- `crates/app/src/download.rs`
- `crates/app/src/main.rs`
- `crates/pipeline/workers/src/model_verifier.rs`
- catalog migration/repository support for current and previous generation ids
- wizard/model UI state and i18n keys
- security/threat-model and model user documentation

## 3A. Approved Pure Phase 1 — Trusted Manifest and Policy

1. Encode Appendix B as typed immutable application data and add a test proving
   exact field parity with the normative artifact.
2. Reject moving revisions, unknown hosts, relative redirects, extra redirects,
   credentials, and manifest/size overflow at the type/validation boundary.
3. Separate provenance status from local byte-integrity status.
4. Define pure URL/redirect/header-policy decisions, including the no-proxy
   construction requirement, without building a production HTTP client.
5. Extend readiness/plan inputs so app-managed files are checked against trusted
   metadata; preserve a clear manual-model state.
6. Add pure tests for manifest parity, host/redirect/header/proxy policy, plan
   actions, and status vocabulary.

Review point: pure manifest/policy implementation review. Stop before schema,
network client, or filesystem work.

## 3B. Post-Amendment Phase — Generation State and Serialization Types

1. Define typed managed-generation ids and lifecycle states.
2. Define catalog records for current/previous ids, activation epoch,
   later-startup validation epoch, and invalid/inactive status.
3. Define `ModelStoreMutationGuard` with shared-reader/exclusive-writer modes
   over `<models-dir>/.model-store.lock`.
4. Enforce lock ordering: model-store guard before catalog transaction.
5. Add pure/state-machine and cross-process lock tests before mutation code.

Review point: schema/locking slice before download or generation promotion.

## 4. Phase 2 — Serialized Generation Stager and Plan Executor

1. Acquire the exclusive cross-process mutation guard with bounded timeout;
   fail closed if unavailable.
2. Replace direct GUI `download::run(dest)` semantics with a worker receiving a
   fresh `ModelReadinessReport`, `DownloadPlan`, and trusted manifest.
3. Preflight the platform path/volume contract, then allocate a unique
   same-filesystem `.staging/<install-id>` directory; never write into current
   or previous generations. On Windows this preflight occurs before staging or
   network transfer.
4. Execute only non-skip actions with maximum concurrency 2 using a production
   client with environment/system proxy discovery disabled.
5. Stream to `.part` files with exact/max byte enforcement and timeouts.
6. Flush/close and verify trusted size/digest for the complete required set.
7. Complete RFC-050 §7.1's platform barriers: Unix renames and syncs `onnx/`
   then the generation root; Windows uses the reviewed write-through rename
   primitive and never claims directory `sync_all`.
8. Promote to a new immutable `generations/<install-id>` using the platform
   barrier: Unix then syncs `.staging/`, `generations/`, and model root;
   Windows uses same-volume, no-replacement `MoveFileExW` with exactly
   `MOVEFILE_WRITE_THROUGH`.
9. Re-verify the immutable generation.
10. While still holding the guard, open one catalog transaction, read current,
    derive previous from that value, and activate the new generation.
11. Emit Ready only after transaction commit and final active-generation check.

Use a local mock HTTP server in tests. Do not rely on external network services
for acceptance evidence.

## 5. Phase 3 — Serialized Recovery and Lifecycle Matrix

1. Increment and record the profile startup epoch under the exclusive guard.
2. Recover/quarantine incomplete staging directories without activating them.
3. Leave complete unreferenced generations inactive.
4. Inject crashes against the platform tables in RFC-050 §7.1: Unix retains
   file flush, nested/parent directory sync, rename, and catalog boundaries;
   Windows uses before/after write-through file, promotion, recovery, and
   quarantine moves. Both platforms inject before/after inactive registration,
   activation, both invalid-current rollback branches, later-startup validation,
   and predecessor release. Do not retain obsolete Windows directory-sync
   points.
5. For invalid `B` and verified previous `A`, atomically produce
   `(current=A, previous=NULL)` and mark `B` invalid; if both fail, produce
   `(NULL, NULL)`.
6. Perform later-startup trusted digest plus tokenizer/ONNX load/dimension
   validation before recording `validated_startup_epoch`.
7. Reject a second activation until current has later-startup validation; an
   initial activation from `current=NULL` has no predecessor to validate.
8. Keep current/previous generations out of cleanup; make previous eligible
   only after current's later-startup validation is durable.
9. Run installer/recovery/rollback/cleanup adversarial interleavings from
   separate processes under the same lock.
10. Run the activation/recovery matrix on Windows as well as Unix-like targets.
11. Make local mock-server shutdown tolerate a client cancelling one concurrent
    transfer after another transfer fails; a cancelled request must not leave a
    fixture waiting forever for an unaccepted connection.
12. Preserve the executed-test count with platform evidence. A command that
    exits successfully after filtering out every requested test is not passing
    evidence for this phase.
13. After one concurrent transfer fails, stop scheduling new transfers but
    drain already-started transfer futures before cleaning the staging
    directory. Do not race Windows file flush/sync work against cancellation and
    recursive removal.
14. On Windows, preserve extended-length absolute path behavior, Unicode, raw
    Win32 error diagnostics, same-volume identity, and local fixed NTFS/ReFS
    policy. Fail closed for UNC/network, redirected network roots, removable or
    unknown media, FAT/exFAT, and cross-volume moves.
15. Use a non-elevated directory junction/reparse fixture, assert
    `FILE_ATTRIBUTE_REPARSE_POINT`, and prove production rejection. Do not skip
    on `ERROR_PRIVILEGE_NOT_HELD` (1314).
16. Derive Windows volume/filesystem identity from a validated existing parent
    or managed-root handle, reject reparse ancestors before trusting that
    identity, and test a lexically local root redirected through a junction.

Review point: generation transaction and injected-crash evidence before GUI
integration.

## 6. Phase 4 — Consent, Localization, and Threat Model

1. Show source/provider, immutable revision, size, license, location, and
   verification status before download.
2. Distinguish app-verified from user-supplied models without alarming copy.
3. Localize all progress and failure states through RFC-052's catalog boundary.
4. Document redirects, request metadata, parser risks, dependency patching,
   size/tensor bounds, and residual risk.

## 7. Required Failure Tests

- already-valid skip;
- missing-file install;
- invalid-file repair;
- interrupted `.part` restart;
- checksum and size mismatch;
- disallowed redirect/host;
- timeout and mid-stream failure;
- concurrent transfer bound;
- staging-directory promotion/rename failure;
- Windows write-through file/directory rename and nonexistent-target failure;
- Windows Unicode, extended-length, malformed/interior-NUL, UNC-policy, volume,
  and filesystem-policy cases;
- non-elevated Windows junction/reparse rejection;
- mutation-lock timeout and crashed-owner release;
- cleanup versus promoted-pre-activation generation interleaving;
- activation deriving previous from commit-time current;
- crash at every activation point;
- complete-set preservation and verified previous-generation rollback;
- exact `(A, NULL)` / `(NULL, NULL)` rollback outcomes;
- abrupt exit immediately before/after verified-previous rollback commit;
- abrupt exit immediately before/after both-invalid rollback commit;
- second-activation rejection before later-startup validation;
- startup-epoch and cleanup-eligibility durability;
- mixed-generation rejection;
- current/previous cleanup protection;
- process restart and final readiness recheck;
- GUI lifecycle compositional proof from the compiled controller/adapter,
  private transaction core against a local mock server, and separately proven
  production-entry wrapper and binding obligations, as specified by Appendix D;
- credential-bearing proxy environment variables cannot influence routing or
  emit proxy authorization.

## 8. Validation

- narrow model/app/worker tests including mock-server tests
- Appendix D's named GUI lifecycle compositional proof; do not relabel any
  injected private-core test as app-layer end-to-end evidence
- focused platform durability-helper tests on Unix and Windows
- nonzero Windows checksum, lifecycle, reparse, contention, and abrupt-exit
  evidence using the platform-specific barrier names
- `cargo test --workspace --lib`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo audit --deny warnings`
- RFC-052 UI policy gates once available
- `git diff --check`

## 9. Stop Conditions

Return to design/security review if the provider cannot offer immutable
artifacts, atomic replacement is unavailable on a target platform without a
new recovery protocol, SQLite cannot durably represent the generation switch,
the cross-process locking contract is unavailable on a target, neither Unix
directory sync nor the reviewed Windows write-through rename can meet its
platform contract, model selection must change, or
implementation would claim that checksums make parsers safe.

## 10. Definition of Done

The GUI executes the reviewed plan, only authenticated complete generations can
activate, the last coherent verified generation survives every simulated
failure, mixed revisions cannot load, manual provenance is represented honestly,
consent and errors are localized, and the updated threat model plus
cross-platform end-to-end evidence pass independent security review.
