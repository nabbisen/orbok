# RFC-061: Catalog Access and the Application Boundary

**Project:** orbok\
**RFC:** 061\
**Title:** Catalog Access and the Application Boundary\
**Status:** Proposed\
**Target milestone:** application correctness and responsiveness\
**Date:** 2026-09-01\
**Related RFCs:** RFC-002 SQLite Catalog Schema and Migration Policy (§5 "one serialized writer path" — this makes it true); RFC-048 Real-Model Benchmark Performance Recovery (§6 is very likely its unmet p99); RFC-056 Hosting the Indexing Scheduler (this shares the catalog it opens); RFC-018 Crash Recovery, Diagnostics and Repair Tools (§8 surfaces what it currently cannot see)

---

## 1. Summary

`crates/data/db/src/catalog.rs` opens with:

> *"Writes are serialized through a single mutex-guarded connection (RFC-002 §5
> 'one serialized writer path')."*

The application calls `bootstrap::open_catalog` **thirteen times in `main.rs`**,
once per UI message family, each constructing a fresh `rusqlite::Connection`
wrapped in its own `Mutex`. The scheduler task opens a fourteenth. The mutex
serializes nothing across handles; SQLite's file locking is the only
serialization; and **no `busy_timeout` is set anywhere in the workspace**, so a
contended write returns `SQLITE_BUSY` immediately rather than retrying.

That error is then discarded. `let _ = scheduler.complete(&job.id, &catalog)`
drops the failure while removing the job from `known` on the same line — so the
catalog row stays `queued`, `rehydrate` re-adds it, and the same file is
extracted and chunked again, each pass inserting more FTS rows.

The same file has four related defects with the same root: the application
boundary does not surface failure. This RFC addresses all of them together,
because they are one design problem.

---

## 2. Motivation

The project already knew. `crates/app/src/scheduler_host/tests.rs:1517`:

> *"both connections share one `Catalog` with no `busy_timeout` pragma set
> (`catalog.rs`) … a `SQLITE_BUSY` returns as an immediate `Err` that this
> slice's `let _ =` call sites silently drop, not a retry"*

Recorded in a test comment during RFC-056's implementation, correctly, and never
turned into work. It is not in `ROADMAP.md`'s debt register. That is the pattern
worth naming: a defect found during implementation and written where only its
finder will read it.

`scheduler_host.rs:60`'s own comment independently records the other half —
*"`scan_and_index_source` writes new `index_jobs` rows directly, on the UI's own
`Catalog` connection"* — which is the fourteenth connection, described accurately,
next to code that assumes one.

---

## 3. Goals

- Make RFC-002 §5's "one serialized writer path" describe what the program does.
- Stop discarding the outcome of state transitions and backend calls.
- Take multi-second blocking work off the GUI event loop.
- Construct the embedding model once per process, not once per search.
- Make a failure to start the indexing subsystem visible.

## 4. Non-Goals

- **Parallel indexing.** Real (single-threaded indexing is ~7 files/s against a
  10 files/s gate) and explicitly out of scope here. It must come *after* this
  RFC: parallelism makes the connection defect load-bearing rather than
  intermittent.
- A full `reduce`/`Effect` refactor of `main.rs`. §9 states the direction and
  scopes the first step; the whole is a separate, larger piece.
- Async SQLite, a connection pool, or a second database. One shared handle is
  the design RFC-002 already specifies.

---

## 5. Decision 1 — one `Catalog` for the process

Open once in `main`, share by `Arc<Catalog>`. Every current `open_catalog` call
site borrows it.

This single change closes four findings at once:

| Closes | How |
|---|---|
| The broken serialization | One connection means the `Mutex` serializes, as documented |
| The per-message migration probe | Each `open_catalog` runs `run_pending`: `CREATE TABLE IF NOT EXISTS schema_migrations` plus six `SELECT EXISTS(…)` probes, per UI message |
| The double-open in `SubmitSearch` | Two opens in one message handler |
| Twelve `if let Ok(catalog) = …` silent-swallow sites | With no fallible open, there is nothing to swallow — "Remove folder" and "Reset catalog" stop being able to silently do nothing |

**Additionally set `busy_timeout`** in `Catalog::from_connection`:

```rust
conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(db_err)?;
```

One shared handle plus a timeout is belt and braces, deliberately: the scheduler
task and the UI still contend at the SQLite level through WAL, and the timeout
is what makes that contention a wait instead of an error.

## 6. Decision 2 — one embedding model for the process

`bootstrap/search.rs:43` calls `create_embedding_model` inside every search:
a 17 MB tokenizer parse, a ~470 MB ONNX protobuf parse, `into_optimized()`,
`into_runnable()` — hundreds of milliseconds to seconds, **on the iced `update`
thread**, before a single token is embedded.

**The correct pattern already exists in this codebase, on the indexing side.**
`embedding_resolution::resolve_embedding_worker_parts` resolves once and the
`EmbeddingWorker` holds the model for the loop's lifetime. The search path
simply does not use it.

Resolve once alongside `state` in `main`; pass a borrow into `run_search`.

**Consequence for RFC-048, stated plainly.** This is very likely the dominant
term in the unmet p99 gate (843.88 ms against a 200 ms threshold), and the
benchmark harness cannot currently see it because it constructs the service
outside its timing loop — which is why keyword-only p99 is green and real-model
p99 is not. RFC-058 §7 fixes the harness. **Fix the harness first**, then this,
then re-measure. Measuring after the fix without fixing the instrument would
produce a green number that means nothing.

## 7. Decision 3 — the GUI thread stops blocking

`iced`'s `update` closure synchronously performs:

- `rfd::FileDialog::pick_folder()` — acknowledged in a comment
- `bootstrap::scan_and_index_source` — a full recursive walk with SHA-256 of
  every file
- `bootstrap::run_search` — including §6's model construction

The window is unresponsive for the duration of each. **The correct pattern is
already in the same file**: `main.rs` uses `iced::Task::perform` with
`AsyncFileDialog` for the RFC-045 picker. Apply it to the expensive paths.

Scope note: `scan_and_index_source` enqueues jobs the hosted scheduler then
drains, so moving it off the update thread is mostly about the walk and the
hashing, not about the indexing itself.

## 8. Decision 4 — failures become visible

Three classes, one mechanism. The `UserNotice` machinery already exists.

**(a) State transitions stop being discarded.** `scheduler_host.rs:212`, `:221`,
`:360`, `:372` — `let _ = scheduler.pause/resume/complete/fail(..)`. The
`complete` case is the live-lock: it drops the error *and* removes the job from
`known`, so the work is redone rather than the write retried. The correct
handling keeps the job in `known` and retries the write:

```rust
match scheduler.complete(&job.id, &catalog) {
    Ok(()) => { known.remove(&job.id); }
    Err(error) => {
        // The work succeeded; the catalog did not record it. Keep the job
        // in `known` so `rehydrate` does not load a second copy, and retry
        // the write on the next iteration rather than redoing the work.
        tracing::warn!(job = job.id.as_str(), %error, "could not record completion");
    }
}
```

This is also how RFC-062's CHECK-constraint failure became an invisible
behaviour change rather than a visible error: on a catalog created by orbok
≤ 0.16.0, `scheduler.pause(&catalog)` fails, `let _ =` eats it, and turning off
background indexing saves the setting and pauses nothing.

**(b) Silent settings writes.** `let _ = persist_theme(..)`,
`persist_text_scale(..)`, `persist_reduced_motion(..)`,
`bootstrap::reset_catalog(..)`, `bootstrap::remove_source(..)`. A failed write is
currently invisible: the UI shows the new value and the next launch shows the old
one. Route to a `Message::BackendError(UserNotice)`.

**(c) The indexing subsystem can fail to start, silently.**
`scheduler_host.rs:104`, `:107`, `:110` — three consecutive
`let Ok(..) = … else { return; }` with no log and no notice. If the runtime
context, the catalog, or the cache cannot be opened, `run` returns, the
subscription completes, and the application looks completely healthy while
indexing nothing, forever. Log at `error!` and emit a notice on each branch.

**(d) Panics.** Thirteen production panic sites and no `std::panic::set_hook`
anywhere. In `iced`, a panic inside `update` or `view` terminates the process.
Four are not defensible and are in scope here:
`main.rs:102` (`"active model store must be authorized"`), `main.rs:193/204/215`
(`"active cache path must be authorized"` ×3 — clicking *Clear snippets* can
terminate the app), and `wizard.rs:56` (`"wizard_view called without active
wizard"` — a panic in `view` is unrecoverable; render a fallback). Convert those
five to error paths and install a panic hook so RFC-018's diagnostics observe
the rest. The remainder (`timeutil`, `chunker`'s guarded `unwrap`s,
`model_delivery`'s infallible `write!`) are acceptable and stay.

## 9. Decision 5 — direction for the app boundary, first step only

`main.rs`'s `update` closure is ~390 lines and is the application's business
logic layer: catalog opens, folder picking, source registration, scanning,
searching, history, cleanup, settings persistence, with effect logic inline in a
`match`. Five of this audit's app-level findings live there.

The project already has the right pattern, extracted from this same closure:
`model_flow::reduce` returns a typed `ModelFlowEffect` and is clean and well
tested.

**In scope here:** `bootstrap` returns `OrbokResult<T>` instead of
`Result<_, Box<dyn Error>>`. That single change is what makes §8 possible —
today the boundary discards `ErrorCategory`'s taxonomy exactly where the UI
needs it to pick an i18n message key, which is why callers fall back to
`if let Ok` and `let _ =` in the first place.

**Out of scope here, recorded as the direction:** extending `reduce`/`Effect` to
the sources, search, cleanup and history message families, leaving `main.rs` as
effect execution only.

---

## 10. Acceptance criteria

Phrased per RFC-058 §5.

1. With indexing active on a several-hundred-file source and a search issued
   concurrently, no job is processed twice — asserted by counting distinct
   `chunk_fts` insertions per file, not by observing that nothing crashed.
2. Starting the application and issuing ten searches constructs the embedding
   model exactly once, observable in the `model_construction_ms` timing field
   (RFC-058 §7) or in a load counter.
3. With a catalog file made read-only, invoking Reset catalog produces a
   user-visible notice; today it silently does nothing.
4. With the data directory made unreadable at startup, the application logs at
   `error!` and shows a notice rather than presenting a healthy UI that never
   indexes.
5. With a source folder containing several thousand files, the window remains
   responsive to input during the initial scan — measured as the update loop
   continuing to process messages, not by impression.
6. `p99` measured through the corrected harness (RFC-058 §7) after §6 lands is
   recorded, whatever its value. This criterion is met by the measurement
   existing, not by it passing — the gate is RFC-048's.
7. Deliberately failing a `scheduler.complete` (test hook) leaves the job in
   `known`, does not re-run the work, and retries the write on the next tick.
8. Clicking Clear snippets with the cache path unavailable produces a notice and
   the process survives.

---

## 11. Open questions

1. **Does `Arc<Catalog>` change the `--check` headless path?** `run_check`
   opens its own catalog and has its own schema-version guard that the GUI lacks
   (RFC-062 §6). It should keep opening its own; it is a separate process
   lifetime. Confirm during implementation.
2. **`busy_timeout` value.** 5 s is the audit's suggestion and is a reasonable
   default for a desktop app where the alternative is an error. Worth revisiting
   once indexing is parallel; a timeout long enough to hide a deadlock is its own
   problem.
3. **Should `main.rs` keep a fallible `open_catalog` at all?** With §5, the
   catalog exists for the process lifetime. The remaining question is what
   happens when the *first* open fails at startup — currently `load_initial_state`
   handles it. Not a design question; flag it if implementation finds otherwise.
