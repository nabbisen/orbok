# RFC-056: Hosting the Indexing Scheduler in the Application

**Project:** orbok\
**RFC:** 056\
**Title:** Hosting the Indexing Scheduler in the Application\
**Status:** Accepted\
**Target milestone:** indexing pipeline conformance\
**Date:** 2026-08-11\
**Accepted:** 2026-08-11 by the project owner\
**Handoff:** [`HANDOFF-056-hosting-the-indexing-scheduler.md`](../handoffs/HANDOFF-056-hosting-the-indexing-scheduler.md)\
**Related RFCs:** RFC-036 Resource-Aware Indexing Scheduler (this hosts it, and does not re-specify it); RFC-049 Portable Runtime Data Isolation (`ProfileModelStore` is the boundary-crossing precedent); RFC-008 §15/§19 embedding job lifecycle; RFC-050 Trusted Atomic Model Delivery (the lease); RFC-039 Privacy Modes

---

## 1. Summary

RFC-036's scheduler exists — 836 lines of policy, accepted and marked
Implemented at v0.17.0 — and has never run in the application. This RFC decides
**how the application hosts it**: where the loop runs, how the RFC-049 profile
boundary and the RFC-050 model lease cross into it, and what happens at
shutdown.

It does not re-specify RFC-036. Every scheduling decision — priority,
backpressure, concurrency limits, pause/resume/cancel, resource awareness —
stays as RFC-036 wrote it.

The immediate driver is that embedding generation is now correct and queued
(RFC-008 §19, landed) but cannot be connected: running it on the synchronous
path blocks the UI for a measured **57.6 s per 400 files**.

## 2. Triggering evidence

### 2.1 Everything except the host already exists

- **Policy:** `crates/pipeline/workers/src/scheduler/` — `dispatch.rs` (303),
  `queue.rs` (231), `job.rs` (210), `limits.rs` (65). `Scheduler::tick()`
  returns `Option<IndexJob>`: it selects the next job by priority, backpressure
  and limits. It executes nothing and references no worker.
- **Background execution:** already in production for RFC-050's model download —
  `main.rs:64` spawns `download::run` on `tokio::spawn`, streaming
  `ModelDeliveryEvent`s to the UI through a channel.
- **Boundary crossing:** `runtime_storage.rs:224-228` already solved this once:

  > A managed-model store sealed to the profile it was constructed for. Owns no
  > reference back into `RuntimeContext`, so it is `Send + 'static` and can cross
  > a `tokio::spawn`/`iced::Task` boundary — but it can only ever be constructed
  > by `RuntimeStorage::model_store`, never from an arbitrary path handed to it
  > directly.

What is missing is the loop that joins them.

### 2.2 The measurement that makes this necessary

Independently reproduced on the same host at `1e14a07`: **400 documents in
57.6 s (143.9 ms/document, 1600 embeddings)**. Extrapolated: ~52 s at 361 files,
~2.4 min at 1,000, ~12 min at 5,000. `scan_and_index_source` is synchronous by
its own doc comment. That is a frozen window, not a slow one.

### 2.3 Two settings currently promise this and do nothing

`OrbokSettings` carries `background_indexing` and `pause_on_battery`. Neither is
read anywhere outside the struct definition. The settings surface offers the
user control over behaviour that does not exist — RFC-036 §12 and §13 are what
make them true.

### 2.4 This is the project's dominant failure mode, not an isolated gap

Built-and-unwired, current inventory: RFC-036's scheduler; `diagnostics.rs`
(zero callers); `mark_embedding_dependents_stale` (zero callers); the two
settings above; and, until this week, `EmbeddingWorker` itself — see RFC-008
§27. Each was accepted, each is real code, none of it runs.

That history is a design input, not a lament. It is the reason §9's acceptance
criteria are written the way they are.

## 3. Decision

The application hosts RFC-036's scheduler in a **long-lived background task,
using the same pattern already proven by the model download**: `tokio::spawn`
owning the scheduler loop, communicating with the UI over a channel.

Work crosses into that task through **profile-sealed, `Send + 'static` handles
constructible only by `RuntimeStorage`**, following `ProfileModelStore` exactly.

`scan_and_index_source` stops performing indexing work synchronously. It
enqueues and returns.

## 4. Required behaviour

1. **The loop.** A spawned task pulls from `Scheduler::tick()` and executes the
   returned job through the existing workers. RFC-036's limits (§9) govern
   concurrency; this RFC does not add its own.
2. **Boundary.** Whatever the task needs — catalog access, cache, model store —
   reaches it as profile-sealed handles in the `ProfileModelStore` mould: no
   reference back into `RuntimeContext`, constructible only through
   `RuntimeStorage`. **No path is handed across as a `Path`/`PathBuf`.**
3. **The RFC-050 lease.** `ResolvedModelDir`'s `_guard` currently lives for the
   duration of a synchronous call. It must now live for as long as the task may
   embed. Whatever owns the loop owns the lease.
4. **UI.** Per RFC-036 §14: partial readiness is searchable, progress copy is
   user-facing rather than queue-depth telemetry. Search must work against
   prepared files while work continues.
5. **Shutdown and recovery.** Per RFC-036 §16: persist job state, never
   transient task handles; on startup `Running → Pending`. Closing the
   application mid-index must not corrupt job state or leave a half-written
   vector.
6. **The two settings become real.** `background_indexing` maps to RFC-036 §12's
   pause; `pause_on_battery` to §13's resource awareness.

## 5. Non-goals

1. **Re-specifying RFC-036.** Priority, backpressure, limits, pause semantics
   and resource policy are its decisions. If implementation finds one of them
   wrong, that is an RFC-036 amendment and a separate conversation.
2. **Changing the scheduler's algorithms** to make hosting easier. If hosting
   requires a policy change, stop — that inverts the dependency.
3. **A new concurrency primitive.** The download pattern exists; use it.
4. **Backfilling existing profiles.** Profiles indexed before this have zero
   embeddings and gain them when their sources are re-scanned. Whether an
   explicit rebuild control is needed is a separate product question.
5. **Keyword retrieval performance.** Independent; still queued at 90.6% of
   search p99.

## 6. Alternatives rejected

**A — Bespoke background thread for indexing, bypassing RFC-036.** Cheapest to
reach working semantic search, and the worst of the three on every axis this
RFC was asked about. It creates a *second* concurrency pattern to audit against
the RFC-049 boundary; it gives no priority, backpressure, cancel or resource
awareness, all of which must then be retrofitted; and it leaves RFC-036's 836
lines unwired while adding more code — feeding §2.4's pattern rather than
reducing it.

**B — Ship synchronously with a progress indicator.** Honest about the wait, but
the wait is minutes on a real corpus, during which the application cannot be
used. It also forecloses nothing and buys nothing: the same hosting work is owed
afterwards.

**C — Leave indexing unwired, keep keyword-only.** Defensible as a deliberate
product state, and it is what ships today. Rejected as a *default* because it is
currently accidental rather than chosen: the settings promise otherwise, the
model is downloaded and verified, and the search path constructs a hybrid
service that scans nothing.

## 7. Security and privacy

**Cancel is a privacy control, not a convenience.** Indexing reads the user's
files. RFC-039 gives the user authority over what orbok touches; without
RFC-036 §12's cancel, "stop indexing now" has no prompt implementation. This is
the strongest security argument for hosting the real scheduler rather than a
bare spawn.

**One boundary-crossing pattern, not two.** The download path's crossing is
already reasoned about and documented. A second, differently-shaped crossing
would need its own audit and would be the natural place for the RFC-049
guarantee to be quietly weakened — the sealed-handle rule in §4.2 exists to
prevent that.

**Nothing gains filesystem reach.** §4.2's prohibition on passing raw paths is
the operative constraint; `ProfileModelStore::models_dir_display` returning a
`String` rather than a `PathBuf` (Review 113 F2) is the precedent to follow.

## 8. Testing requirements

1. Adding a source returns control promptly, measured, with the embedding work
   still outstanding.
2. Embeddings accumulate while the UI remains responsive.
3. Search returns results for prepared files while preparation continues
   (RFC-036 §14.2).
4. Pausing via `background_indexing` stops new work; resuming continues it.
5. Shutdown mid-index, restart, and the run resumes with no corrupt job state
   and no partial vector (RFC-036 §16).
6. Cancelling a source's work leaves no orphaned jobs (RFC-036 §18.6).
7. No model installed ⇒ behaviour matches today's keyword-only path exactly.
8. **Every one of these exercises the shipped application's path**, not a
   worker invoked directly by a test. That is the specific failure this RFC
   exists to correct and the way its predecessors were missed.

## 9. Acceptance criteria — written as behaviour, deliberately

RFC-008 §27 records why: its own criteria were phrased as capabilities
("chunks *can be* embedded locally"), so every one was truthfully checkable
while the shipped product did none of it. RFC-009 §24 records the sharper case,
where the broken state read as a satisfied requirement. **Criteria below name
observable behaviour of the running application.** None of them can be satisfied
by code that exists but is not called.

- [ ] Adding a source of ≥400 files returns control to the UI in under 2 seconds.
- [ ] After that source finishes preparing, `embeddings` is non-zero and equals
      the chunk count for its files.
- [ ] While preparation is in progress, a search returns results and the UI
      accepts input.
- [ ] With `background_indexing` off, no new indexing work starts; turning it on
      resumes it.
- [ ] With `pause_on_battery` on and the machine on battery, indexing pauses.
- [ ] Killing the application mid-preparation and restarting resumes it, with no
      job left in `running` and no embedding row for an unfinished chunk.
- [ ] Removing a source while it is preparing leaves no queued job for it.
- [ ] With no model installed, all of the above behave as they do today, and no
      unbounded job growth occurs across repeated scans.

## 10. Risks

**RFC-036 has never been exercised.** Its §18 item 4 — "Embedding work yields to
active search" — was untestable outside unit tests, because embedding never ran.
Marked Implemented at v0.17.0 under the same conditions that produced RFC-008
§27. **Expect wiring to surface gaps in the scheduler itself**, and treat each as
an RFC-036 amendment rather than something to absorb here. That is a reason to
sequence this deliberately, not a reason to avoid it.

**Scope creep toward RFC-036 edits** is the likeliest way this goes wrong; §5.2
is the guard.

## 11. Note to the reviewer

Every claim in §2.1 is from the current tree: the line counts, `tick()`'s
signature, `main.rs:64`'s spawn, and the `runtime_storage.rs` comment quoted
verbatim. §2.2's measurement was reproduced by the architect independently of
the dev team's run, agreeing within 1.3%.

The architect previously told the owner that wiring RFC-036 would be "the
largest piece of work." That was wrong, and this RFC is written on the corrected
understanding: the policy engine and the background pattern both already exist,
and the missing piece is the join.
