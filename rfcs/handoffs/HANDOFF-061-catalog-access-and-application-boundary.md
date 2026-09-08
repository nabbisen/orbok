# Implementation Handoff — RFC-061: Catalog Access and the Application Boundary

**Project:** orbok\
**RFC:** 061\
**Lifecycle stage:** Accepted 2026-09-02; Amendment 1 (measured baseline) 2026-09-03; criterion 9 re-drafted 2026-09-04. Unstarted.\
**Primary owner:** `crates/app/src/main.rs`, `.../bootstrap/*`, `.../scheduler_host.rs`, `crates/data/db/src/catalog.rs`\
**RFC:** [`../accepted/061-catalog-access-and-application-boundary.md`](../accepted/061-catalog-access-and-application-boundary.md)

> **Scope rule:** This RFC does **not** parallelise indexing. §4 defers that
> deliberately, and it must stay deferred until this lands — parallelism makes
> the connection defect load-bearing rather than intermittent. If a slice tempts
> you toward `rayon`, `spawn_blocking` on the dispatch loop, or honouring
> `SchedulerLimits`, stop.

---

## 1. Current state, re-measured 2026-09-09 — the RFC's numbers have moved

The RFC was written against `3e26f92`. Three of its figures have changed, and
one grew:

| | RFC says | Now |
|---|---:|---:|
| `bootstrap::open_catalog` calls in `main.rs` | 13 | **14** |
| `if let Ok(catalog) = …` sites | 12 | **12** |
| `busy_timeout` in production | 0 | **0** (the single grep hit is a test comment at `scheduler_host/tests.rs:1726`) |
| `let _ =` in `scheduler_host.rs` | 4 named | **8** |

**The 14th `open_catalog` was added by Task 035**, wiring source refresh. That is
the point worth carrying into the work: **the defect grows while it is
described.** Every new UI message that needs the catalog opens a fourteenth,
fifteenth connection, because the shape invites it.

Two of the eight `let _ =` are also new — `:485` and `:506`, `jobs.set_status`,
added by Task 035's rehydrate fix and flagged in Review 200 §5. They are
defensible (the `list_blocked` loop retries) and they are still the pattern this
RFC removes.

## 2. Slices, ordered by dependency rather than by RFC section number

### Slice 1 — one `Catalog` for the process (§5)

Open once in `main`, share by `Arc<Catalog>`; every current call site borrows it.
Add `conn.busy_timeout(Duration::from_secs(5))` in `Catalog::from_connection`.

**This is the slice that pays for the RFC.** It closes, in one change: the broken
serialization, the per-message migration probe (six `SELECT EXISTS` per UI
message), the double-open in `SubmitSearch`, and **twelve `if let Ok(catalog)`
silent-swallow sites — because with no fallible open there is nothing to
swallow.** "Remove folder" and "Reset catalog" stop being able to quietly do
nothing.

Do the `busy_timeout` in the same slice even though one shared handle makes it
belt-and-braces: the scheduler task and the UI still meet at the SQLite level
through WAL, and the timeout is what turns that contention into a wait instead of
an error.

### Slice 2 — `bootstrap` returns `OrbokResult<T>` (§9's in-scope half)

Five modules return `Result<_, Box<dyn std::error::Error>>` today:
`preferences.rs` (7), `sources.rs` (4), `startup.rs` (4), `cleanup.rs` (3),
`search.rs` (2).

Mechanical, and **it is the prerequisite for Slice 3**: the boundary currently
discards `ErrorCategory` exactly where the UI needs it to choose an i18n message
key, which is why callers fall back to `let _ =` in the first place.

### Slice 3 — failures become visible (§8)

Four kinds, one mechanism — `UserNotice` already exists.

**(a) State transitions.** `scheduler_host.rs:229/238/397/409`. The `complete`
case at `:397` is the live-lock: it drops the error *and* removes the job from
`known`, so the work is redone rather than the write retried. RFC-061 §8(a) has
the replacement code; use it.

**(b) Settings writes.** `let _ = persist_theme/persist_text_scale/persist_reduced_motion`,
`reset_catalog`, `remove_source` in `main.rs`. A failed write currently shows the
new value and restores the old one next launch.

**(c) Silent start failure.** `scheduler_host.rs:104/107/110` — three
consecutive `let Ok(..) = … else { return; }` with no log. If the runtime
context, catalog or cache cannot be opened, the app looks healthy and indexes
nothing, forever.

**(d) Panics.** Five convert to error paths: `main.rs:104` (model store),
`main.rs:195/206/217` (cache path — *clicking "Clear snippets" can terminate the
app*), and `views/wizard.rs:56` (a panic in `view` is unrecoverable; render a
fallback). Install `std::panic::set_hook` — **there is none anywhere in the tree**
— so RFC-018's diagnostics observe the rest. `timeutil`, `chunker`'s guarded
`unwrap`s and `model_delivery`'s infallible `write!` stay.

### Slice 4 — one embedding model for the process (§6)

`bootstrap/search.rs:44` calls `create_embedding_model` inside every search: a
17 MB tokenizer parse, a ~470 MB ONNX protobuf parse, `into_optimized()`,
`into_runnable()` — on the iced `update` thread.

**The pattern already exists in this codebase**:
`embedding_resolution::resolve_embedding_worker_parts` resolves once and the
`EmbeddingWorker` holds it for the loop's lifetime. The search path simply does
not use it.

### Slice 5 — off the update thread (§7), last

`main.rs` calls `run_search` synchronously at `:314`, `:398`, `:436`, and
`pick_folder()` at `:156` — with a comment already acknowledging it blocks.
`iced::Task::perform` is used in the same file at `:123` and `:299`, so the
pattern is present and simply not applied to the expensive paths.

**Last, because it changes concurrency**, and Slices 1 and 3 are what make a
concurrent failure visible rather than silent.

## 3. The measurement — fix the instrument before taking the reading

Acceptance criterion 9 asks for a reading against Amendment 1's baseline. **The
instrument is censored and will lie if you do not fix it first.**

`scheduler_host/tests.rs`: `overall_start` at `:1781`, the work wrapped in
`tokio::time::timeout(Duration::from_secs(300))` at `:1801`, and
`overall = overall_start.elapsed()` read at `:1810` — **after the timeout
returns**. So on timeout the number is ≈300 s by construction. Two Windows runs
two days apart reported **300.0358954 s** and **300.037572 s** — two milliseconds
apart, which is a ceiling, not variance.

**The uncensored shape already exists twelve lines above**, in the same file:
`background_indexing_baseline_with_no_concurrent_access` (`:1706`) drains to
completion and simply prints `start.elapsed()` — no timeout wrapper, no
assertion.

**Give the concurrent test the same shape.** Drain until done, print elapsed,
and either drop the assertion or keep a very generous one purely as a
hang-detector (an hour, say — enough that it can only fire on a genuine wedge).
**That is not widening a threshold to make a test pass**: criterion 9 wants a
number, not a verdict, and the assertion is what has been preventing the number
from existing.

Take the reading on all three platforms **after Slices 1 and 4**, and record it
whatever it says. If Windows does not drop toward Linux's 44.79 s, Amendment 1's
hypothesis was wrong and the cost is elsewhere — which matters more, not less,
because §4 defers parallel indexing until this is understood.

## 4. The two things most likely to go wrong

**`Arc<Catalog>` and `--check`.** `run_check` opens its own catalog and has a
schema-version guard the GUI lacks (RFC-062 §6). It should keep opening its own —
separate process lifetime. Confirm rather than assume; `from_connection` is also
used by `open_in_memory` for tests.

**Slice 3(a) and the rehydrate fix.** `:485`/`:506`'s `let _ = jobs.set_status`
were added deliberately by Task 035 and the `list_blocked` loop retries behind
them. Converting them needs care not to introduce a second recovery path — a
`tracing::warn!` is the whole fix, per Review 200 §5.

## 5. Definition of done

1. One `Catalog` for the process; `busy_timeout` set; zero `if let Ok(catalog)`
   remaining in `main.rs`.
2. `bootstrap` returns `OrbokResult<T>` from all five modules.
3. The four state transitions, the settings writes and the three start-failure
   branches all surface; a panic hook is installed; the five named `expect` sites
   are error paths.
4. One model per process, observable in `model_construction_ms` (RFC-058 §7's
   field, now built) or a load counter.
5. The latency test is uncensored per §3, and a reading is recorded on all three
   platforms against Amendment 1's baseline — **whatever it says**.
6. RFC-061 §10's criteria 1–8 hold, each with the mutation that shows it can
   fail.

## 6. Stop conditions

- A slice appears to need parallel indexing, `spawn_blocking` on dispatch, or
  `SchedulerLimits`. That is §4's deferral and it stays deferred.
- Slice 1 changes `--check`'s behaviour (§4).
- The uncensored reading shows Windows **not** improving. Report it; do not
  re-run hunting for a better number. A negative result is a pass for criterion 9
  and is the more useful outcome.
- Converting a `let _ =` in `scheduler_host.rs` requires a new recovery path
  rather than a log line.

## 7. Not in scope

Extending `reduce`/`Effect` to the remaining message families (§9's out-of-scope
half — recorded there as direction); parallel indexing (§4); the FTS row leak,
which RFC-059 §6 owns and which Slice 1 makes rarer rather than fixes.
