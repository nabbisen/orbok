# Implementation Handoff — RFC-059 Slice 6: the extraction-cache bound, at scheduler idle

**Project:** orbok\
**RFC:** 059 — Amendment 2 §2b, criterion 10\
**Lifecycle stage:** Accepted; Slices 1–5 shipped (`5754b54`…`58076f8`). This is the last slice before `done/`.\
**Primary owner:** `crates/app/src/scheduler_host.rs` (the hook), `crates/pipeline/workers/src/cleanup_service.rs` (the trim)\
**RFC:** [`../accepted/059-erasure-completeness-and-cache-lifetime.md`](../accepted/059-erasure-completeness-and-cache-lifetime.md)

---

## 1. Why this slice exists

`OrbokCacheNamespace::cleanup_time_entry_cap()` returns 20,000 and nothing
reads it. The write-time cap was withdrawn because it evicted under a running
pipeline (Amendment 1 §2a.2); the cleanup-time trim was removed because the
cleanup action now erases outright and a trim there had nothing to trim
(Review 215 §3). The owner kept the bound and chose its one safe home: **when
nothing is indexing.** Until this lands, RFC-059 §7 asserts a bound the code
does not have, and the RFC stays in `accepted/`.

## 2. Where the hook goes — one place, already there

`scheduler_host.rs:365-385`. The hosting loop calls `scheduler.tick()`; on
`None` it `rehydrate`s from `index_jobs` and ticks again; on a second `None` it
flushes a pending health report and `sleep(IDLE_POLL)`s (300 ms). **That second
`None` is idle by the definition criterion 10 needs**: the in-memory queue is
empty *and* rehydrate found no queued rows in the catalog. Nothing can be
mid-flight — `tick()` returned nothing to run.

Two constraints, both from the RFC:

- **Once per transition, not once per poll.** The idle branch runs every
  300 ms for as long as the app sits idle. Keep a `trimmed_since_idle: bool`
  next to `health_report_pending`; set it false whenever `tick()` returns a
  job; trim only when it is false, then set it true. A trim per poll would
  hammer `list_entries()` on a large cache for nothing.
- **Never with a job queued.** The placement above guarantees it, but
  criterion 10's second half — *"with any index job queued, no entry is
  evicted"* — is the assertion to write first and break deliberately: put the
  trim one branch up (after the first `None`, before `rehydrate`) and watch
  the test fail because a catalog-queued job was still pending.

## 3. The trim

In `cleanup_service.rs`, next to `erase_engine_namespace`:

```rust
pub fn trim_engine_namespace_to<T>(engine: &CacheEngine<T>, cap: usize) -> OrbokResult<u64>
```

`list_entries()` → sort by `(last_accessed_at, updated_at)` ascending → take
`len - cap` → `remove()` each → `shrink_database()` **only if something was
removed** → return the count. Public API only — Review 214 §2's raw-SQL version
is the thing not to bring back. Expose one wrapper on `CleanupService` (or
`ProfileCache`, whichever the host already holds) that opens `ExtractSegments`
with `default_engine_options()` and calls it with `cleanup_time_entry_cap()`;
the host must not learn engine internals.

Log at `info!` with `entries_evicted` and `cap`, like `erase` does. Do not
surface a notice: this is maintenance, not a user action.

## 4. Tests — criterion 10, both halves

`crates/app/src/scheduler_host/tests.rs` already drives the real hosting loop
with a real catalog and cache (`search_latency_while_background_indexing_is_running`
is the template). Two tests, one shape:

1. **Idle trims.** Seed `ExtractSegments` with `cap + N` entries (write through
   the real engine; a small test cap via a constructor parameter or a
   `#[cfg(test)]` override on the host — *not* by editing `namespace.rs`).
   Enqueue nothing. Run the loop until it has been idle for two polls. Assert
   `entry_count() == cap`, that the survivors are the most recently accessed
   `cap` entries, and that a third idle poll evicts nothing further.
2. **Queued blocks the trim.** Same seed, plus one `Extract` job queued in
   `index_jobs` for a file that does not exist (it will fail, which is fine —
   what matters is that it was queued). Assert the count is still `cap + N`
   at every poll until that job has left the queue, and only then drops.

**Break it and watch it fail** — the RFC says so in criterion 10's own text.
Move the trim above `rehydrate`; test 2 must go red.

## 5. Definition of done

- `cleanup_time_entry_cap()` has exactly one reader, in the host's idle branch.
- Both tests pass; the deliberate misplacement was observed failing.
- No raw SQL, no second connection, no `VACUUM` outside `shrink_database()`.
- README: the sentence "an automatic size bound is designed but not yet wired
  to anything that runs it" becomes true-and-shorter — say when it runs.
- Closure record row for criterion 10; then, with 1–10 all evidenced, RFC-059
  moves to `done/` in the same commit.

## 6. Stop conditions

- `list_entries()` on a 20,000-entry namespace takes long enough on the idle
  path to be felt in the UI (the hosting task is off the update thread, but
  it shares the catalog `Mutex`). Measure it once at 20,000 synthetic entries
  and report the number; if it is more than a few hundred milliseconds, stop
  — the trim needs to page, and that changes the shape.
- The idle branch turns out to be reachable while a job is `Running` in
  another task (it should not be — one hosting loop, one worker at a time —
  but if RFC-061's later slices have changed that, say so before building).
