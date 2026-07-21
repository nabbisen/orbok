# RFC-050 Appendix D — GUI Lifecycle Integration Design

**Status:** Phase 4 design review input
**RFC:** [`../proposed/050-trusted-atomic-model-delivery.md`](../proposed/050-trusted-atomic-model-delivery.md)
**Handoff:** [`../handoffs/HANDOFF-050-trusted-atomic-model-delivery.md`](../handoffs/HANDOFF-050-trusted-atomic-model-delivery.md)
**Consent/threat delta:** [`APPENDIX-C-rfc050-phase4-consent-threat-model.md`](APPENDIX-C-rfc050-phase4-consent-threat-model.md)
**Localization policy:** [RFC-052](../proposed/052-ui-localization-and-design-gate-compliance.md) and [HANDOFF-052](../handoffs/HANDOFF-052-ui-localization-and-design-gate-compliance.md)

This appendix designs the next bounded Phase 4 slice authorized after
Architecture Review 096. It does not implement the design, accept Phase 4,
change the reviewed worker protocol, or establish release readiness.

## 1. Objective and boundaries

The slice must make the GUI-triggered lifecycle independently testable from:

1. accepted consent;
2. guarded worker start;
3. typed progress or failure events;
4. authenticated managed-generation completion;
5. explicit Ready acceptance;
6. persistence behavior; and
7. ready model resolution after restart.

It must also localize every progress/failure string introduced or touched by
that path. It must not add an arbitrary download URL, a runtime test override,
an environment-variable bypass, a public arbitrary-manifest installer, or a
second production downloader.

The existing Windows durability/storage implementation and Review 093 waiver
are outside this slice. A worker-core change that affects transaction,
durability, storage, or path behavior is a stop condition rather than implied
authorization.

## 2. Current flow and evidence gaps

Current production flow:

| Step | Current owner | Current behavior | Gap |
|---|---|---|---|
| Offer | `orbok-ui` | `DownloadModel` opens typed consent | Accepted by Review 096 |
| Confirm | `orbok` main closure | Guard checks `DownloadConsent`, then synchronously sends `DownloadStarted` | Logic is embedded in the large iced closure |
| Install | `crates/app/src/download.rs` | Opens catalog/store and calls `install_default_model` | Only guard has app-level test |
| Progress | app adapter | Converts `ModelDeliveryEvent::FileProgress` to `DownloadFileProgress` | Raw logical name and ad-hoc formatting reach UI |
| Success | app adapter/UI | Sends `DownloadAllComplete`, then shows managed Ready | No app-to-worker local-mock evidence |
| Failure | app adapter/UI | Sends `DownloadFailed(String)` and returns silently to `NotConfigured` | Raw string crosses boundary; failure is not visibly recoverable |
| Accept | main closure/UI | Persists manual path or clears stale managed setting, then UI clears Ready | UI state accepts `WizardAccept` even outside Ready; persistence failure does not block transition |
| Restart | bootstrap | Managed recovery/load validation, guarded catalog resolution, lightweight readiness, provenance projection | Covered compositionally, not from a GUI-triggered local-mock install |

Current model progress rendering also contains untranslated literals for file
position, byte units, percentage punctuation, model summary, logical artifact
name, and Back. RFC-043 catalog keys for model progress/failure exist but are
not connected to the wizard path. Those strings cannot be carried forward as
exceptions in the new lifecycle surface.

## 3. Required invariants

The implementation review must prove all of these:

1. `ConfirmModelDownload` starts work only while `DownloadConsent` is active.
2. The state changes to `Downloading` before a task is spawned; duplicate or
   stale confirmation cannot start another task.
3. Production still invokes `orbok_workers::install_default_model` with the
   Appendix B trust root and production client. No test configuration is
   selectable at runtime.
4. The UI receives typed progress, typed safe failure categories, or an
   authenticated generation outcome; it never receives a worker error string,
   URL, query string, or local error path.
5. Only worker success can create managed Ready. Progress cannot create Ready.
6. `WizardAccept` changes capability/provenance only from the exact Ready
   identity for which the required persistence attempt succeeds.
7. Persistence failure leaves Ready visible and exposes a localized retryable
   problem; it cannot silently claim acceptance.
8. A managed restart derives `AppManaged` from the guarded catalog current only
   after startup recovery/load validation and readiness. Manual restart remains
   `UserSupplied`.
9. Failure and restart preserve the worker guarantees for the previous coherent
   generation. The app does not delete, promote, activate, or repair files.
10. All progress/failure copy uses RFC-052 typed keys or parameterized
    formatters in English and Japanese.
11. The adapter waits for the authoritative worker to become quiescent before
    selecting one terminal outcome. It drains admitted progress before that
    outcome and never abandons mutation because presentation delivery failed.

## 4. Proposed application boundary

### 4.1 Extract a model-flow controller

Move model-specific effect decisions out of the iced closure into a small app
module, tentatively `crates/app/src/model_flow.rs`. The controller remains an
application concern and owns no worker protocol internals.

Its pure decision vocabulary is:

```text
ModelFlowEffect
├── None
├── StartManagedDownload
└── PersistReady {
      ready_id,
      persistence_attempt_id,
      model_dir,
      provenance
    }
```

`ReadyId` identifies one entry into Ready. `PersistenceAttemptId` identifies
one write attempt for that Ready identity. Both are app-internal opaque,
monotonically allocated values, not paths or hashes reconstructed from UI
copy. Ready retains its exact model directory, provenance, and persistence
status: `Idle`, `InFlight(attempt_id)`, or `Failed`.

Required behavior:

- `ConfirmModelDownload` + active consent:
  synchronously apply `DownloadStarted`, return `StartManagedDownload`.
- `ConfirmModelDownload` outside consent:
  return `None` and do not mutate state.
- `WizardAccept` + Ready in `Idle` or `Failed`:
  allocate a fresh persistence attempt, mark it `InFlight`, and return
  `PersistReady` without clearing Ready.
- `WizardAccept` + Ready in `InFlight`:
  return `None`; duplicate clicks cannot start concurrent writes.
- `WizardAccept` outside Ready:
  return `None` and do not change capability, provenance, or wizard state.
- persistence completion carries `ready_id`, `persistence_attempt_id`, the
  expected model directory and provenance, and a typed success/failure result;
- a completion applies only if every carried identity field matches the active
  Ready and its `InFlight` attempt;
- matching success sets capability/provenance and clears Ready;
- matching failure retains Ready, sets its status to `Failed`, and exposes a
  typed localized acceptance failure; and
- stale, duplicate, or mismatched completions are no-ops.

The controller is a pure reducer. The iced effect executor, not the controller,
owns `ModelPreferenceStore` I/O and returns the correlated completion message.
Persistence retry is the ordinary `WizardAccept` action from the same Ready in
`Failed`; it allocates a new persistence attempt and cannot return
`StartManagedDownload`. The executor must use only the model directory and
provenance carried by the effect, then echo those values in its result.

### 4.2 Keep the production installer binding explicit

Refactor the adapter into:

```text
download::run(paths, tx)
  -> open Catalog + ManagedModelStore
  -> run_with_installer(..., |catalog, store, events| {
         install_default_model(catalog, store, events)
     })
  -> translate events/outcome to typed UI messages
```

`run` is the only production entry used by the controller. The generic
`run_with_installer` is `pub(crate)` and accepts a boxed lifetime-bound future;
it does not accept URLs, manifests, clients, or paths other than the already
computed catalog/store paths.

Production code must contain a direct, reviewable reference from `run` to
`install_default_model`. A source/AST assertion may supplement review but does
not replace any component or wrapper obligation in §4.3.

### 4.3 Named compositional proof; no public test transport

The exact Appendix B production entry cannot successfully fetch tiny local
HTTP fixtures: it correctly fixes immutable HTTPS provider URLs, host policy,
digests, sizes, and model identity. Weakening that binding for tests would
violate the RFC. RFC-050 §12 and HANDOFF-050 therefore explicitly define the
following named compositional proof instead of requiring or claiming one
app-layer end-to-end localhost execution.

| Proof component | Compiled boundary | Obligations proved |
|---|---|---|
| Controller tests | The real app `model_flow` module | Consent and duplicate-start guards; UI state transitions; correlated Ready persistence; restart projection |
| Adapter tests | The real app `download` module calling `run_with_installer` | Invocation, typed event translation, terminal arbitration, progress drain, closed UI receiver behavior |
| Worker local-mock tests | The private production transaction core used by `install_default_model` | HTTP transfer, limits, integrity, staging, promotion, activation, catalog outcome, prior-generation preservation |
| Production-wrapper tests | `install_default_model` and its existing private helpers | Trusted-manifest validation, managed-store preflight, exclusive lock, guarded snapshot/readiness plan, already-ready verification, and production-client construction |
| Production-binding evidence | The compiled app adapter plus manifest/source assertion and review | `download::run` directly calls `install_default_model`; Appendix B parity, redirect, host, no-proxy, and client policy remain sealed |

The app tests compile and call the app crate's own controller and adapter;
neither file is copied or source-included into the worker crate. The worker
local-mock test remains in the worker crate so it can use the already reviewed
private fixture seam. The shared public worker types and the direct production
call join these component boundaries. The implementation review must map every
wrapper obligation above to an observed test or explicit source review; private
core evidence cannot stand in for an omitted wrapper obligation.

This construction has mandatory constraints:

- no worker test-support feature or app/UI worker dependency;
- no exported arbitrary-manifest/client function;
- no runtime or release environment switch;
- no copied controller or adapter logic;
- no claim that an injected adapter or private-core test is app-layer
  end-to-end evidence; and
- no claim that the immutable production worker ran against localhost.

If implementation cannot compile these exact component boundaries, or an
unproved wrapper obligation appears, stop for design review. Literal execution
of `install_default_model` against localhost would require a separate secure
test-routing design, not an override or loosened production policy.

## 5. Typed UI lifecycle contract

### 5.1 Progress

Replace raw logical-name presentation with a closed UI enum:

```text
ModelArtifact
├── Tokenizer
└── OnnxModel
```

The app adapter exhaustively maps the two Appendix B logical names. An unknown
worker logical name becomes a typed internal adapter failure; it is not shown
verbatim.

`DownloadFileProgress` carries `ModelArtifact`, exact bytes, total bytes,
files-done, and files-total. Rendering uses typed parameterized formatters for:

- localized artifact label;
- one-based file position;
- current/total byte count with localized units; and
- integer percentage with locale-safe surrounding copy.

The formatter must define the zero-total and completed-file edge cases. The
view must not compute `files_done + 1` beyond `files_total`.

### 5.2 Failure

Replace `DownloadFailed(String)` with a closed safe category:

```text
ModelDeliveryFailure
├── StoreUnavailable
├── Connection
├── Verification
├── LocalStorage
└── InternalState
```

Proposed worker-to-app mapping:

| Worker error | UI category |
|---|---|
| `StoreUnavailable`, `StoreBusy` | `StoreUnavailable` |
| `Network` | `Connection` |
| `TrustPolicy`, `Plan`, `TransferLimit`, `Integrity`, `FinalCheck` | `Verification` |
| `Filesystem`, `Catalog` | `LocalStorage` |
| adapter failure before authoritative worker start | `InternalState` |

The adapter may log only the existing safe worker category. UI messages contain
no raw `Display` text. An unknown artifact is an adapter-contract diagnostic,
not an early terminal result; §5.4 defines its arbitration with the
authoritative worker outcome.

Failure becomes a visible wizard state retaining the reviewed consent facts,
original setup return state, and safe category. Retry reopens
`DownloadConsent` with those exact facts and requires a fresh
`ConfirmModelDownload` action; it must not synthesize metadata, silently switch
source/revision, or start network work directly. Keyword-only continuation
remains available.

### 5.3 Acceptance failure

Persistence failure is distinct from delivery failure because the generation
may already be active coherently. Show localized copy that the model is ready
but the preference could not be saved, keep the Ready screen, and allow retry.
Do not rerun the installer merely because settings persistence failed.

### 5.4 Adapter terminal arbitration

`run_with_installer` owns one installer future, the sole event-channel
receiver, and one UI sender. Its state machine is:

```text
Running
  ├── valid worker event -> translate/send if UI is open; remain Running
  ├── invalid event -> record InternalState diagnostic, suppress event; remain Running
  ├── UI send closes -> mark UI closed; remain Running
  └── worker resolves -> Draining(authoritative result)

Draining
  ├── drain every event already queued when the worker resolved
  ├── apply the same translation/suppression rules without blocking worker state
  └── queue empty and worker-owned senders dropped -> Terminal

Terminal
  └── select exactly one terminal outcome from the authoritative worker result
```

Installer completion is the quiescence boundary: the worker contract forbids
detached event producers, so all worker-owned event senders are dropped when
the future resolves. The adapter drains queued events before constructing the
terminal message. It never polls progress after the terminal selection and
therefore cannot deliver post-terminal progress.

The worker result always wins terminal arbitration. Success yields managed
Ready even if an earlier unknown event was suppressed; failure yields the
exhaustively mapped worker failure. The adapter records an `InternalState`
diagnostic for an unknown event but cannot report an early failure, drop the
installer, or contradict a generation the worker later activates.
`InternalState` is a user-visible terminal category only for an adapter failure
that prevents starting an authoritative worker; it does not replace a worker
result after start.

If the UI receiver closes, the adapter stops sending progress but continues to
poll the installer through quiescence and drains its event queue. It selects
one terminal outcome and makes at most one terminal send attempt when the UI
channel remains open; a known-closed channel has no deliverable terminal
message. Channel closure never cancels or detaches the worker. Tests distinguish
exactly one selected terminal outcome from delivery to a closed receiver.

## 6. Persistence and restart seam

Introduce an app-internal repository interface used only by the iced effect
executor:

```text
ModelPreferenceStore
├── accept_user_supplied(path)
└── accept_app_managed(data_dir)
```

The pure controller never calls this interface. The production executor
delegates to the existing settings helpers and sends a completion containing
the effect's `ReadyId`, `PersistenceAttemptId`, model directory, and provenance.
A temp-backed executor test records calls and can inject failure. It does not
mock catalog activation; activation remains exclusively worker-owned.

Move the pure projection from `(VerifyOutcome, resolved provenance)` to
`(capability, wizard, active provenance)` into the small model-flow module so it
can be used by bootstrap and tested without the complete GUI closure. Bootstrap
still performs, in order:

1. managed startup recovery/load validation;
2. guarded managed/manual resolution;
3. readiness verification; and
4. pure UI projection.

The integration evidence may use the worker's existing private lifecycle test
validator for its tiny local fixture, then feed the resulting catalog snapshot
through the same app projection. It must not claim that the tiny fixture equals
the production Appendix B model. Production trust-root parity and real
load/probe tests remain separate governing evidence.

## 7. Compositional lifecycle evidence sequence

No single test is called app-layer end-to-end evidence. The named proof must
link these observed component sequences through the exact shared message and
worker types:

1. App controller tests start from `NotConfigured`, show reviewed consent, and
   prove one synchronous `Downloading` transition and one start effect.
2. App adapter tests use an injected deterministic installer future to produce
   typed progress and one authoritative terminal result through the compiled
   adapter. They feed those messages through the real controller/UI state and
   prove only terminal success creates the Ready identity.
3. App persistence tests accept that Ready identity through the real
   controller and temp executor, then prove matched completion produces
   `Hybrid` + `AppManaged`. The real startup projection is separately exercised
   from a guarded managed-resolution result.
4. Worker tests invoke the private production transaction core against the
   existing local mock fixture, observe progress and terminal success, validate
   the resulting catalog generation through later-startup lifecycle checks,
   and assert only expected fixture requests occurred.
5. Production-wrapper and binding tests/review prove all §4.3 obligations that
   the fixture-capable core begins below.

The proof report must name each test and show the type/call boundary joining it
to the next component. It must not narrate the separate executions as one
literal runtime trace.

Companion failure evidence must prove, across the corresponding components:

- controller/adapter failure creates no Ready transition;
- the worker local-mock failure preserves the previous catalog current where
  applicable;
- localized typed failure and retry/keyword-only actions remain visible;
- unknown events do not cancel an active worker or override its terminal
  result; and
- no raw worker error, path, URL, document, or query enters UI state.

## 8. Required tests and evidence matrix

| Requirement | Minimum evidence |
|---|---|
| Guard and duplicate confirmation | Pure controller unit test |
| Defensive `WizardAccept` | UI state/controller unit tests outside Ready and in Ready |
| Progress mapping | Adapter tests for both artifacts, zero/complete edges, En/Ja formatters |
| Failure mapping | Exhaustive worker-error-to-UI-category test |
| Visible localized failure | En/Ja simulator test with retry and keyword-only actions |
| Persistence success/failure | Matching temp executor tests; failure retains Ready |
| Persistence correlation | Stale, duplicate, mismatched Ready/attempt/path/provenance completions are no-ops; retry allocates a fresh attempt and cannot start installer |
| Terminal arbitration | Deterministic queued-progress-at-completion, active-install unknown event, closed UI receiver, drain-before-terminal, and exactly-one-selected-terminal tests |
| App controller + adapter | Tests compile and call the real app modules; no copied/source-included logic |
| Worker local mock | Private production transaction core success/failure tests against the existing fixture |
| Success lifecycle | Named component evidence for consent → progress → Ready → correlated accept → restart projection |
| Failure lifecycle | Named component evidence for consent → typed failure, no Ready, prior coherent generation preserved |
| Production wrapper | Tests/review for every wrapper obligation enumerated in §4.3 |
| Production binding | Existing and refreshed Appendix B parity, transport, no-proxy tests plus direct `run` binding assertion/review |
| Dependency boundary | `cargo tree`/manifest assertion: no app/UI worker dependency or normal cycle |

## 9. Implementation staging and review stops

Implement only after this appendix is accepted, in two separately reviewed
patches:

### Slice A — Controller, typed states, and localization

- extract controller and startup projection;
- factor and test `run_with_installer` terminal arbitration while retaining the
  direct production call to `install_default_model`;
- guard `WizardAccept`;
- add typed progress/failure/persistence states;
- connect En/Ja formatters and visible recovery UI;
- add focused UI/app tests.

This is a screen-bounded RFC-050 migration of only the model lifecycle strings
identified in §2. It neither claims nor replaces RFC-052 Phase 1's required
repository-wide inventory/classification review, and it must not expand into a
bulk cross-screen catalog migration.

Mandatory review stop before any worker test-core refactor.

### Slice B — Worker/core evidence and production binding

- factor the private worker core without changing production decisions;
- add worker success/failure/local-mock/restart evidence;
- refresh every production-wrapper obligation in §4.3;
- prove the app adapter's direct production binding;
- prove dependency and release surfaces remain sealed.

Mandatory architecture/security review before Phase 4 consolidation.

## 10. Validation gates

Each implementation slice must run, as applicable:

- focused `orbok-ui`, `orbok`, `orbok-models`, and `orbok-workers` tests;
- nonzero local-mock selections;
- `cargo test --workspace --lib` with loopback available;
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
- `cargo audit --deny warnings --no-fetch`;
- `cargo fmt --all -- --check`;
- `git diff HEAD --check` (or `git diff --cached --check` for a wholly staged
  review patch);
- RFC lifecycle check;
- dependency-tree check proving workers gain no app/UI dependency;
- package/release hermeticity checks proving worker normal/release builds and
  repository packaging require no app/UI test source;
- a Windows compile check for Slice B's worker refactor, even when Review 093
  analysis finds no fresh Windows runtime execution necessary;
- RFC-052 gates when they become available.

No fresh Windows execution is required for a UI/app/test-only Slice A. Slice B
must explicitly diff every affected call path against Review 093 and run a
Windows compile check. It must return to Windows runtime review/evidence if its
worker refactor changes any durability, filesystem, catalog transaction,
cancellation/drain, path, storage, failure, or lifecycle decision rather than
only factoring the already-tested private core.

## 11. Stop conditions and non-goals

Stop and return to design/security review if implementation would:

- allow runtime selection of a test manifest/client/server;
- export arbitrary trusted metadata or an HTTP-capable installer;
- weaken Appendix B validation or production HTTPS/host/no-proxy policy;
- let the app activate, promote, delete, or repair generations;
- treat an injected app installer as worker local-mock evidence or narrate the
  named compositional proof as one literal execution;
- clear Ready after failed persistence;
- expose raw worker errors or paths to UI;
- change worker durability/storage/lifecycle decisions; or
- require a production dependency from workers to UI/app.

Out of scope: automatic update, cancel/resume protocol, alternative providers,
new model selection, full RFC-052 repository-wide migration, packaging,
performance, and release readiness.

## 12. Design review questions

Independent review must explicitly decide:

1. Do the RFC-050/HANDOFF-050 amendments and §4.3 obligation map define an
   honest, sufficient compositional replacement for the former app-layer
   localhost requirement?
2. Does the paired Ready/persistence-attempt identity contract prevent stale,
   duplicate, mismatched, and failed persistence from claiming capability or
   provenance?
3. Does terminal arbitration guarantee worker quiescence, drain admitted
   progress, prevent post-terminal events, and handle unknown events/closed UI
   receivers without abandoning mutation?
4. Are the two implementation slices small enough, and is the Slice A review
   stop correctly placed before worker refactoring?
5. Are the Windows compile/package gates and Review 093 stop conditions
   sufficient for the proposed Slice B refactor?
