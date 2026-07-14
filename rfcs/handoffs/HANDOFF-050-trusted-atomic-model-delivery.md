# Implementation Handoff — RFC-050: Trusted Atomic Model Delivery

**Project:** orbok  
**RFC:** 050  
**Lifecycle stage:** Design + handoff  
**Primary owners:** model registry/readiness, app download worker, security  
**RFC:** [`../proposed/050-trusted-atomic-model-delivery.md`](../proposed/050-trusted-atomic-model-delivery.md)

**Trust root:** [`../appendices/APPENDIX-B-default-model-trust-root.md`](../appendices/APPENDIX-B-default-model-trust-root.md)

> **Scope rule:** Preserve valid model files until a trusted replacement has
> passed verification. Never derive expected integrity metadata from the bytes
> being accepted.

## 1. Approval Gate

Do not implement production network or activation code until Appendix B and
RFC-050's generation protocol receive independent security/design approval.
Phase 1 may encode the already-reviewed data only after that approval; it may
not discover or substitute trust values at runtime.

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

## 3. Phase 1 — Trusted Metadata and State Model

1. Encode Appendix B as typed immutable application data and add a test proving
   exact field parity with the normative artifact.
2. Reject moving revisions, unknown hosts, relative redirects, extra redirects,
   credentials, and manifest/size overflow at the type/validation boundary.
3. Separate provenance status from local byte-integrity status.
4. Define typed managed-generation ids/states and catalog current/previous
   activation records.
5. Extend readiness/plan inputs so app-managed files are checked against trusted
   metadata; preserve a clear manual-model state.
6. Add pure tests for manifest parity, host/redirect policy, generation state,
   plan actions, and status vocabulary.

Review point: security/design review of trust root and UI claims before network
worker changes.

## 4. Phase 2 — Generation Stager and Plan Executor

1. Replace direct GUI `download::run(dest)` semantics with a worker receiving a
   fresh `ModelReadinessReport`, `DownloadPlan`, and trusted manifest.
2. Allocate a unique same-filesystem `.staging/<install-id>` directory; never
   write into current or previous generations.
3. Execute only non-skip actions with maximum concurrency 2.
4. Stream to `.part` files with exact/max byte enforcement and timeouts.
5. Flush/close and verify trusted size/digest for the complete required set.
6. Write the manifest snapshot and completion marker, flush the staged tree,
   and rename it to a new immutable `generations/<install-id>` directory.
7. Re-verify the immutable generation, then update current and previous ids in
   one durable catalog transaction.
8. Emit Ready only after transaction commit and final active-generation check.
9. Retain the previous generation through a successful subsequent startup.

Use a local mock HTTP server in tests. Do not rely on external network services
for acceptance evidence.

## 5. Phase 3 — Recovery and Crash Matrix

1. Recover/quarantine incomplete staging directories without activating them.
2. Leave complete unreferenced generations inactive.
3. Inject crashes before/after each file flush, staged rename, parent sync, and
   catalog transaction boundary.
4. Validate current generation at startup; roll back atomically only to the
   recorded previous generation after full verification.
5. Keep current/previous generations out of cleanup plans.
6. Run the activation/recovery matrix on Windows as well as Unix-like targets.

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
- crash at every activation point;
- complete-set preservation and verified previous-generation rollback;
- mixed-generation rejection;
- current/previous cleanup protection;
- process restart and final readiness recheck;
- GUI-triggered end-to-end worker path.

## 8. Validation

- narrow model/app/worker tests including mock-server tests
- `cargo test --workspace --lib`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo audit --deny warnings`
- RFC-052 UI policy gates once available
- `git diff --check`

## 9. Stop Conditions

Return to design/security review if the provider cannot offer immutable
artifacts, atomic replacement is unavailable on a target platform without a
new recovery protocol, SQLite cannot durably represent the generation switch,
model selection must change, or implementation would claim that checksums make
parsers safe.

## 10. Definition of Done

The GUI executes the reviewed plan, only authenticated complete generations can
activate, the last coherent verified generation survives every simulated
failure, mixed revisions cannot load, manual provenance is represented honestly,
consent and errors are localized, and the updated threat model plus
cross-platform end-to-end evidence pass independent security review.
