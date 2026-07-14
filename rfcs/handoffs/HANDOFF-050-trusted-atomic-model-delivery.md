# Implementation Handoff — RFC-050: Trusted Atomic Model Delivery

**Project:** orbok  
**RFC:** 050  
**Lifecycle stage:** Design + handoff  
**Primary owners:** model registry/readiness, app download worker, security  
**RFC:** [`../proposed/050-trusted-atomic-model-delivery.md`](../proposed/050-trusted-atomic-model-delivery.md)

> **Scope rule:** Preserve valid model files until a trusted replacement has
> passed verification. Never derive expected integrity metadata from the bytes
> being accepted.

## 1. Expected Change Surface

- `crates/search/models/src/readiness.rs`
- `crates/search/models/src/download_plan.rs`
- a reviewed trusted-manifest module/asset in `orbok-models` or `orbok-app`
- `crates/app/src/download.rs`
- `crates/app/src/main.rs`
- `crates/pipeline/workers/src/model_verifier.rs`
- wizard/model UI state and i18n keys
- security/threat-model and model user documentation

## 2. Phase 1 — Trusted Metadata and State Model

1. Define the embedded/reviewed manifest schema and the default model entry.
2. Record immutable revision URLs, SHA-256 digests, size bounds, identity,
   dimension, and license.
3. Separate provenance status from local byte-integrity status.
4. Extend readiness/plan inputs so app-managed files are checked against trusted
   metadata; preserve a clear manual-model state.
5. Add pure tests for manifest parsing, plan actions, and status vocabulary.

Review point: security/design review of trust root and UI claims before network
worker changes.

## 3. Phase 2 — Atomic Plan Executor

1. Replace direct GUI `download::run(dest)` semantics with a worker receiving a
   fresh `ModelReadinessReport`, `DownloadPlan`, and trusted manifest.
2. Execute only non-skip actions with maximum concurrency 2.
3. Stream to same-filesystem `.part` files with byte limits and timeouts.
4. Flush/close, then verify trusted size/digest.
5. Promote atomically. Preserve/restore an existing valid final file on all
   replacement failures.
6. Re-run readiness/deep verification before emitting Ready.
7. Clean or safely ignore stale partials on restart.

Use a local mock HTTP server in tests. Do not rely on external network services
for acceptance evidence.

## 4. Phase 3 — Consent, Localization, and Threat Model

1. Show source/provider, immutable revision, size, license, location, and
   verification status before download.
2. Distinguish app-verified from user-supplied models without alarming copy.
3. Localize all progress and failure states through RFC-052's catalog boundary.
4. Document redirects, request metadata, parser risks, dependency patching,
   size/tensor bounds, and residual risk.

## 5. Required Failure Tests

- already-valid skip;
- missing-file install;
- invalid-file repair;
- interrupted `.part` restart;
- checksum and size mismatch;
- disallowed redirect/host;
- timeout and mid-stream failure;
- concurrent transfer bound;
- promotion/rename failure;
- valid-final preservation and rollback;
- process restart and final readiness recheck;
- GUI-triggered end-to-end worker path.

## 6. Validation

- narrow model/app/worker tests including mock-server tests
- `cargo test --workspace --lib`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo audit --deny warnings`
- RFC-052 UI policy gates once available
- `git diff --check`

## 7. Stop Conditions

Return to design/security review if the provider cannot offer immutable
artifacts, atomic replacement is unavailable on a target platform without a
new recovery protocol, model selection must change, or implementation would
claim that checksums make parsers safe.

## 8. Definition of Done

The GUI executes the reviewed plan, only authenticated temporary files reach
final paths, valid files survive every simulated failure, manual provenance is
represented honestly, consent and errors are localized, and the updated threat
model plus end-to-end evidence pass independent security review.

