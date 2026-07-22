# RFC-050 Appendix E — Phase 4 Compositional Proof Report

**Status:** Slice B implementation accepted at `5fb085c`; Phase 4 consolidation pending
**RFC:** [`../proposed/050-trusted-atomic-model-delivery.md`](../proposed/050-trusted-atomic-model-delivery.md)  
**Handoff:** [`../handoffs/HANDOFF-050-trusted-atomic-model-delivery.md`](../handoffs/HANDOFF-050-trusted-atomic-model-delivery.md)  
**Trust root:** [`APPENDIX-B-default-model-trust-root.md`](APPENDIX-B-default-model-trust-root.md)  
**Consent/threat delta:** [`APPENDIX-C-rfc050-phase4-consent-threat-model.md`](APPENDIX-C-rfc050-phase4-consent-threat-model.md)  
**Integration design:** [`APPENDIX-D-rfc050-gui-lifecycle-integration-design.md`](APPENDIX-D-rfc050-gui-lifecycle-integration-design.md)  
**Accepted implementation:** `5fb085cc17ada0f6f85a537e8dcde99b98866369`

This report records the named compositional proof required by RFC-050 §12,
HANDOFF-050, and Appendix D §4.3. It joins independently compiled evidence; it
does not claim one literal GUI-to-localhost execution or that the immutable
production worker contacted a loopback server.

Architecture Review 101 authorized this bounded test/documentation Slice B,
and independent Architecture Review 102 accepted the resulting implementation,
which the owner committed at the revision above. This report does not by itself
complete Phase 4, move the RFC lifecycle, expand Review 093, or establish
release readiness.

## 1. Proof boundaries

| Component | Compiled boundary | Evidence | Claim |
|---|---|---|---|
| Controller | Real `orbok` `model_flow` module | `compiled_adapter_controller_and_executor_complete_managed_success`, `compiled_adapter_failure_never_creates_ready`, and the focused reducer matrix | Consent/duplicate guards, typed managed success/failure, correlated Ready persistence, and projection |
| Adapter | Real `orbok` `download` module and `run_with_installer` | Adapter invocation, typed translation, FIFO drain, unknown-event, closed-receiver, and terminal-arbitration tests | The app waits for worker quiescence, drains admitted progress, selects one typed terminal result, and does not abandon worker mutation when presentation closes |
| Private worker transaction | Private `model_delivery::execute_generation`, shared by production and fixtures | `mock_server_install_promotes_complete_generation_and_activates_it`, `checksum_failure_never_promotes_or_activates`, and the existing delivery/crash matrix detailed in §§2.1–2.3 | Loopback transfer, exact integrity, bounded concurrency, staging, durable promotion, activation, compensation, coherent-prior preservation, and later-startup catalog retention |
| Production wrapper | Public `install_default_model` and existing private helpers | Dynamic helper/component tests plus the source/dataflow map in §3 | Sealed manifest validation, preflight, lock, guarded planning, already-ready verification, and production-client construction remain on the production entry |
| Production binding | Compiled app/worker crates plus direct source/dataflow review | The two exact links in §4 and Appendix B parity/policy evidence | Production remains directly bound to `install_default_model`, the sealed manifest/client, and the same private transaction core; no runtime test route exists |

These components share public worker types and direct production call links.
They are not relabeled as one end-to-end runtime trace.

## 2. Private transaction evidence

### 2.1 Successful loopback generation and later startup

`model_delivery::tests::mock_server_install_promotes_complete_generation_and_activates_it`
calls the exact private `execute_generation` function used by production. Its
test-only manifest and client point to a loopback `MockServer`; neither is
exported or selectable by a normal/release build.

The test asserts:

- exactly two recognized fixture requests and a nonzero typed progress set;
- progress for both closed logical artifacts, with positive exact final bytes;
- maximum observed concurrency of two;
- the returned directory is exactly `generations/<returned generation id>`;
- tokenizer and ONNX bytes exactly equal the fixtures and their SHA-256 values
  equal the fixture manifest;
- exact `trusted-manifest.json` and `COMPLETE` metadata;
- recursive absence of `.part` files;
- catalog current equals the exact returned identity; and
- a later `run_managed_model_startup_with` transition records a positive
  startup epoch, retains that exact current, does not roll back, and leaves no
  previous generation.

The tiny restart validator checks the fixture path, both fixture payloads, the
complete marker, and serialized fixture manifest. It proves traversal through
the real startup epoch/catalog/current-validation seam. It does **not** prove
production trusted bytes, real tokenizer/ONNX loading, output dimension, or
the public startup entry.

Those production properties remain separate:

- `model_lifecycle::run_managed_model_startup` directly passes
  `DEFAULT_TRUSTED_MODEL.manifest_id` and `trusted_generation_loads` to the
  private lifecycle seam;
- `trusted_generation_loads` requires real-directory ancestry, production
  trusted bytes, and `embedding_generation_loads`; and
- `real_tokenizer_onnx_load_and_output_dimension_are_checked` dynamically
  exercises tokenizer/ONNX loading and expected output dimension.

### 2.2 Failure preserves a validated coherent current

`model_delivery::tests::checksum_failure_never_promotes_or_activates` first
creates and activates a complete tiny prior generation, then validates it
through a later startup using the same exact-byte/metadata fixture validator.
It next runs `execute_generation` against a same-length corrupt model response.

The test asserts an `Integrity` result, the exact prior identity remains the
sole catalog current and sole generation record, the exact prior directory is
the sole promoted directory, and staging is empty. Thus a failed candidate
does not promote or activate and does not disturb the already validated
coherent generation.

### 2.3 Existing transaction matrix retained

The focused delivery selection also includes:

- concurrent failure drain before cleanup;
- mock-server shutdown with a cancelled pending request;
- promotion rename failure before registration/activation;
- trusted skip without a network request;
- missing marker, corrupt manifest, and catalog-identity rejection;
- post-commit restoration or both-invalid compensation;
- exact-size header mismatch and overflow;
- omitted content length with exact verification;
- timeout and midstream disconnect;
- credential-bearing proxy environment isolation; and
- abrupt process exit at each durability boundary.

Observed selection: 19 tests discovered, 17 passed, and 2 declared
separate-process helpers ignored by the parent tests. The parent abrupt-exit
and proxy tests passed and invoke their helpers explicitly.

## 3. Production-wrapper obligation map

| Obligation | Production source/dataflow | Dynamic evidence boundary |
|---|---|---|
| Appendix B validation | `install_default_model` first calls `DEFAULT_TRUSTED_MODEL.validate()` | `typed_manifest_exactly_matches_appendix_b_json`; invalid path/digest, duplicate/overflow, and production-policy tests |
| Managed-store preflight | The wrapper calls `preflight_managed_store` before lock/catalog/planning; the private core repeats preflight before mutation | Focused `model_durability` platform/policy tests and lifecycle/delivery preflight cases |
| Exclusive locking | The wrapper acquires `store.acquire_exclusive(LOCK_TIMEOUT)` and retains `guard` through final confirmation | `separate_process_lock_mode_matrix_is_enforced`, `crashed_process_releases_lock_without_deleting_lock_file`, and guarded repository tests |
| Guarded snapshot/readiness plan | `load_exclusive(&guard)` derives the current source; `check_app_managed_model_readiness` feeds `build_download_plan` | RFC-043 readiness/plan suites, including exact trusted bytes, pinned metadata, forged/cross-manifest rejection, and maximum concurrency |
| Already-ready verification | Ready plus catalog current calls `verify_ready_current`, which checks record identity and `verify_generation_validity` | `ready_current_rejects_missing_complete_marker_and_corrupt_manifest` covers marker, manifest, and catalog identity |
| Production client | The wrapper constructs only `production_client(&DEFAULT_TRUSTED_MODEL)` | Trust tests cover initial/redirect hosts, credentials, HTTPS/port rules, redirect limit and header classification; source review confirms HTTPS-only, no proxy, no referer, connect/request timeouts, and custom redirect validation |

The wrapper ordering and propagation are source/dataflow evidence. Local
fixture execution does not replace these obligations, and no real-provider
network request is required or claimed.

## 4. Exact production binding links

### 4.1 Compiled app to public worker

`crates/app/src/download.rs::run` opens the production catalog, constructs
`ManagedModelStore::default_embedding`, and directly passes its event sender to
`orbok_workers::install_default_model` through the compiled
`run_with_installer` adapter. The call contains no URL, manifest, client, or
test selector.

The real adapter/controller tests compile this app module and its worker types.
Source/dataflow review supplies the direct production-call fact that cannot be
successfully redirected to a tiny fixture without weakening Appendix B.

### 4.2 Public worker to private transaction

`crates/pipeline/workers/src/model_delivery.rs::install_default_model` directly
calls the same private `execute_generation` exercised in §2. It passes:

- the production store and held exclusive guard;
- `ManagedGenerationRepository` bound to the production catalog;
- the source derived from the guarded current snapshot;
- the plan derived from production readiness;
- `DEFAULT_TRUSTED_MODEL`;
- `production_client(&DEFAULT_TRUSTED_MODEL)`; and
- the app-provided typed event sender.

Both hooks are no-ops in production. There is no exported arbitrary
manifest/client transaction function and no normal/release selector for the
loopback fixture.

## 5. Dependency, build, and package boundary

`orbok-workers` normal dependencies contain worker/model/data/search crates and
shared libraries, not `orbok` or `orbok-ui`. Its dev dependencies contain
`orbok-bench` and test libraries, not app/UI crates. No app/controller source
is copied or `include!`-expanded into the worker crate.

This report does not claim that the complete-workspace archive excludes app/UI
source. Its hermeticity claim is narrower: the worker normal/release dependency
graph and packaged worker source require no copied/included app/UI test logic
and expose no runtime test override. Gate results and archive inspection are
recorded in §7.

## 6. Review 093 and platform analysis

The worker diff is wholly below the existing `#[cfg(test)] mod tests` boundary
in `model_delivery.rs`. It strengthens two tests and the private mock server.
Production code before that boundary is unchanged. No `model_lifecycle.rs`,
durability helper, Cargo manifest, lockfile, platform helper, or worker call
site changes.

Therefore the patch does not alter or reorder:

- raw reparse probing or supported-storage policy;
- namespace/path handling or volume identity;
- durable rename/sync behavior;
- delivery, recovery, quarantine, or lifecycle preflight routing;
- catalog transaction decisions;
- cancellation/drain or failure mapping; or
- any Review 093 waiver condition.

The Windows GNU all-target check required by Reviews 098/101 is compilation
evidence only. It is not Windows runtime evidence. A production change,
contradictory Windows result, or change to any condition above would revoke the
waiver and require a renewed review plus Windows runtime evidence.

## 7. Observed gates

The following evidence was observed on the Slice B working tree:

- `cargo test -p orbok-workers model_delivery::tests --lib -- --nocapture
  --test-threads=1` — 17 passed, 2 explicit separate-process helpers ignored,
  137 filtered out; the parent proxy and abrupt-exit tests passed;
- `cargo test -p orbok-workers model_lifecycle::tests --lib -- --nocapture
  --test-threads=1` — 11 passed, 1 explicit helper ignored, 144 filtered out;
  the real tokenizer/ONNX load and output-dimension test passed;
- `cargo test -p orbok-workers model_durability::tests --lib -- --nocapture`
  — 3 Linux tests passed, 153 filtered out;
- `cargo test -p orbok-models --lib --locked` — 51 passed, including manifest
  parity/policy, redirect, readiness/plan, generation, and lock selections;
- `cargo test -p orbok --bin orbok --locked` — 19 passed, including the real
  adapter/controller joins and startup provenance selections;
- `cargo test -p orbok-ui --lib --locked` — 92 passed, including model consent,
  typed progress/failure, persistence retry, and bilingual view selections;
- `env TMPDIR="$PWD/.git-exclude/tmp" cargo test --workspace --lib` — every
  reported library suite passed; workers reported 153 passed and 3 declared
  helpers ignored;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
  passed;
- `cargo audit --deny warnings --no-fetch` — passed after scanning 697 locked
  dependencies against 1,166 locally loaded advisories;
- `cargo fmt --all -- --check` — passed;
- `git diff HEAD --check` — passed;
- `bash scripts/check-rfc-lifecycle.sh` — `rfc lifecycle gate: ok`;
- `cargo tree -p orbok-workers --edges normal --prefix none` — passed and
  contained no `orbok` or `orbok-ui` node;
- `cargo tree -p orbok-workers --edges dev --prefix none` — passed; the direct
  dev tree was `orbok-bench`, `dirs`, `prost`, `tempfile`, `tract-onnx`, and
  `zip`, with no app/UI node;
- `cargo build -p orbok-workers --release --locked` — passed;
- `cargo check -p orbok-workers --target x86_64-pc-windows-gnu --all-targets
  --locked` — passed after the host MinGW-w64 compiler was installed and the
  build was allowed its required temporary writes; this is compile evidence,
  not Windows runtime evidence; and
- `bash scripts/package.sh 0.24.0-rfc050-sliceb-review` — created a 367-entry
  archive whose checksum verified; the worker manifest/source and this
  appendix were present; `.git-exclude`, `target`, `dist`, and `Cargo.lock`
  were absent; and the packaged worker delivery source contained no
  `include!`, `orbok_ui`, `crates/app`, or `run_with_installer` reference.

The Slice B package and checksum remain disposable ignored historical review
evidence under `dist/`. They are not release artifacts.

Hunk inspection confirms every worker change is below the existing
`#[cfg(test)]` boundary. No mandatory gate remains unrun for the bounded Slice
B implementation review.

## 8. Limitations and non-claims

- The tiny fixture does not prove production model bytes, real provider
  transport, backend load, output dimension, or public startup by itself.
- No immutable production entry ran against localhost.
- No Windows runtime test was run for this test/documentation-only slice.
- The unsupported-storage physical fixture remains governed only by Review
  093's strict waiver.
- No manual English/Japanese GUI session or real provider download/load is
  claimed.
- Phase 4 consolidation, RFC lifecycle movement, and release readiness remain
  pending.
