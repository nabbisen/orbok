# Implementation Handoff — RFC-056: Hosting the Indexing Scheduler

**Project:** orbok\
**RFC:** 056\
**Lifecycle stage:** Accepted 2026-08-11; implementation not started\
**Primary owner:** `crates/app` runtime hosting; `orbok-workers` scheduler integration\
**RFC:** [`../accepted/056-hosting-the-indexing-scheduler.md`](../accepted/056-hosting-the-indexing-scheduler.md)

> **Scope rule:** This wires RFC-036's scheduler into the application. It does
> **not** change RFC-036's policy — priority, backpressure, limits, pause
> semantics, resource rules. If hosting seems to require a policy change, that
> inverts the dependency: stop and report. Every such finding is an RFC-036
> amendment, and RFC-056 §10 says to expect several.

## 1. What already exists — do not rebuild any of it

- **Policy engine.** `crates/pipeline/workers/src/scheduler/` — `dispatch.rs`,
  `queue.rs`, `job.rs`, `limits.rs`. `Scheduler::tick() -> Option<IndexJob>`
  selects the next job. It executes nothing.
- **Background execution pattern.** `crates/app/src/main.rs:64` —
  `tokio::spawn(download::run(model_store, catalog, tx))`, with
  `crates/app/src/download.rs` streaming events to the UI over a channel. This
  is the shape to follow.
- **Boundary-crossing precedent.** `crates/app/src/runtime_storage.rs:224-228`,
  `ProfileModelStore`: sealed to its profile, owns no reference into
  `RuntimeContext`, therefore `Send + 'static`, and constructible **only** by
  `RuntimeStorage::model_store`.
- **The embedding job.** Already enqueued after chunking (RFC-008 §19, landed at
  `d198713`) and already fails as `model_missing` when no worker is present.
  Nothing about job creation needs changing.

## 2. Slices, ordered by risk rather than by user value

### Slice 1 — hosting, without embedding

Spawn the loop, execute extract/chunk jobs through it, and get shutdown and
recovery right. **Do not connect embedding yet.**

Note that this slice will not visibly improve responsiveness: extraction,
chunking and keyword indexing together measured **1,335 ms per 1,000 documents**
(Task 011). They are already fast. That is the point — this slice proves the
hosting architecture while the expensive part is still out of the picture, so a
failure here is unambiguously a hosting failure.

Validate it on RFC-036 §16's recovery behaviour and on jobs actually flowing
through `tick()`, not on UI smoothness.

### Slice 2 — connect embedding

Add the `EmbeddingWorker` and the RFC-050 lease. This is where RFC-056 §9's
first three criteria become measurable, and where the 143.9 ms/document cost
arrives.

### Slice 3 — settings, pause, cancel

`background_indexing` → RFC-036 §12 pause. `pause_on_battery` → §13 resource
awareness. Source removal cancels queued work (§18.6).

Both settings exist in `OrbokSettings` and are read nowhere today. This slice is
what makes them honest.

**Two items carried in from earlier reviews. Recorded here rather than left in
review threads, because that is how they get lost:**

- **RFC-036 §20.2 — the backpressure drop.** `Scheduler::fail`'s retry branch
  skips `queue.push` when the target queue is full but still calls
  `set_status(Queued)`, so the catalog records a job that no longer exists in
  memory and rehydration will not reload it. It lives in the same queue-state
  machinery this slice wires for pause/resume, which is why RFC-036 §20's
  "whoever next touches the backpressure path" resolves to Slice 3
  (Review 171 §5).
- **§3.2b — settle what the 48 ms actually is.** Sample the same search *after*
  indexing completes: full catalog, no concurrent writes. Stays near 48 ms ⇒ the
  cost is catalog size and there is no contention story. Falls toward 270 µs ⇒
  the concurrent writes are real and worth designing against. A handful of lines
  in `search_latency_while_background_indexing_is_running`, which you have just
  written (Review 172 §3).

### Slice 4 — the UI half (added 2026-08-12; my omission)

**RFC-056 §4.4 was assigned to no slice when this handoff was written.** That was
an error, and it matters more now than it did then: Slice 1 deferred indexing and
the scan-routing follow-up (`c3e535e`) deferred discovery too, so a user who adds
a folder now sees *nothing happen* until background work reports back. The
behaviour is correct and `scan_and_index_source`'s doc comment describes it
accurately. What is missing is anything telling the user preparation is underway.

RFC-036 §14 already specifies this, including the copy:

> **§14.1** `Preparing "Documents" for search` / `124 files ready. You can search
> now.` — and explicitly *not* `Indexing queue depth: 412`.
>
> **§14.2** Search must work for prepared files while background work continues.

So this is conformance, not design. Scope:

1. Progress surfaced from the `Message::HealthUpdated` events the host already
   emits — no new plumbing needed.
2. §14.1's copy, through the RFC-031 i18n catalog like every other visible
   string. Note `scheduler_host.rs` is currently in the i18n gate's
   `EXCLUDED_FILES` on the grounds that its only literal is a `tracing` line
   (`5b2a57c`); adding user-visible copy there would make that classification
   wrong, so put the copy in the UI layer where it belongs.
3. §14.2 — confirm search works against prepared files mid-run. Slice 1's
   `search_latency_while_background_indexing_is_running` already exercises the
   mechanism; this is about the user-facing guarantee.

**Ordering:** independent of Slices 2 and 3, and safe to take before either. It
is the slice a user would notice first, which is an argument for not leaving it
last.

`Message::ScanCompleted` is also worth revisiting here: it now fires when a scan
is *enqueued*, and `state.rs:725` already handles it identically to
`HealthUpdated`, so it may simply be redundant (Review 163 §3).

## 3. The two things most likely to go wrong

**3.1 — The RFC-049 boundary.** Everything the task needs must reach it as a
profile-sealed handle in the `ProfileModelStore` mould. **No `Path` or `PathBuf`
crosses the spawn.** If you find yourself wanting to pass one, that is the
signal to add a sealed accessor on `RuntimeStorage` instead. `ProfileModelStore::models_dir_display`
returning a `String` rather than a `PathBuf` (Review 113 F2) is the precedent for
why even display paths are typed away from filesystem APIs.

**3.2 — Catalog contention, which I have partly checked for you.** `Catalog` is
`Mutex<Connection>`, so it is shareable, and two things make this workable:

- WAL journal mode is on (`catalog.rs:46`).
- `embedding.rs` never calls `lock()`, so the mutex is not held across ONNX
  inference — repository calls take and release it individually.

What I have **not** checked, and you should: whether any single repository
operation holds the guard long enough to stall a UI search. `insert_bundle`
writing many chunks in one transaction is the obvious candidate. **Measure a
search's latency while indexing is running**; if it degrades noticeably, report
the number rather than working around it — a second connection is a real option
under WAL, but it is a design change and belongs in a conversation.

> **Correction (2026-08-12, after Slice 2 — Review 172 §3). The question above
> is wrong and was mine.**
>
> **That guard is never shared.** `scheduler_host::run` opens its own catalog
> (`scheduler_host.rs:58`), and the module's own comment states that
> `scan_and_index_source` writes *"on the UI's own `Catalog` connection."* Two
> connections, two mutexes, by construction — so no repository operation on the
> indexing side can hold a guard the UI is waiting on. The "second connection
> under WAL" I offered as a possible remedy is what the code already does.
>
> The architecture answers the concern. The question was badly asked, not the
> design badly built.
>
> Slice 2 measured search latency under indexing anyway and found **~48 ms
> average / ~56 ms max, against a ~270 µs Slice 1 baseline**. That number is real
> and its *cause is unestablished*: WAL-level effects across the two
> connections, catalog growth over a 45 s sampling window versus Slice 1's
> 1.1 s, and residual scheduling artefact from the test's blocking sleep are all
> consistent with it, and the test separates none of them. **Do not quote it as
> "indexing makes search 180× slower"** until §3.2b settles which.

## 4. Testing — the requirement that matters most

RFC-056 §8.8: **every test exercises the shipped application's path**, not a
worker invoked directly.

That is not pedantry here. RFC-008 and RFC-036 were both marked Implemented on
the strength of tests that called workers directly while the application called
nothing — see RFC-008 §27. A test that constructs a `Scheduler` and drives it
by hand would pass identically whether or not `main.rs` ever spawns one.

RFC-056 §9's criteria are written as observable behaviour for the same reason.
Work against those, and if one of them cannot be tested through the app's own
path, say so — that is a finding about the app's testability, not a licence to
test a layer down.

## 5. Verification

- The usual workspace gates and three CI legs.
- Slice 2 onward: the real-model measurement, same host and command as
  `.git-exclude/evidence/rfc008-task013-phase2-blocking/`, for comparison
  against the 57.6 s synchronous baseline.
- RFC-048's cosine test and Task 012's sibling-chunk test must both still pass —
  neither should be affected, and if either moves, something unintended reached
  the embedding path.

## 6. Stop conditions

1. Hosting appears to require an RFC-036 policy change (§scope rule).
2. A `Path`/`PathBuf` seems to need to cross the spawn boundary (§3.1).
3. Search latency degrades measurably while indexing runs (§3.2).
4. An RFC-036 behaviour turns out not to work as specified — expected per
   RFC-056 §10; report it as an amendment candidate and continue if the slice
   can proceed without it.
5. A §9 acceptance criterion cannot be exercised through the application's own
   path (§4).

## 7. Not in scope

Backfilling existing profiles' embeddings; keyword retrieval performance; any
RFC-048 optimisation; `diagnostics.rs`'s zero-caller problem, which is real but
separate.
