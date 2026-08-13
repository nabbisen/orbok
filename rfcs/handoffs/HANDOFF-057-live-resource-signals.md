# Implementation Handoff — RFC-057: Live Resource Signals for the Indexing Scheduler

**Project:** orbok\
**RFC:** 057\
**Lifecycle stage:** Accepted 2026-08-13; implementation not started\
**Primary owner:** `crates/app` scheduler host and UI wiring; `orbok-workers` scheduler transitions\
**RFC:** [`../accepted/057-live-resource-signals.md`](../accepted/057-live-resource-signals.md)

> **Scope rule:** This builds *signal sources* and the path that carries them.
> It does **not** change what any `ResourceMode` does — that is RFC-036 §13's,
> and it is already implemented. If a source seems to require changing the
> policy, stop and report: that is an RFC-036 amendment, same rule as RFC-056.

## 1. What already exists — do not rebuild any of it

- **`ResourceMode`** (`crates/pipeline/workers/src/scheduler/job.rs:200-208`) —
  `Normal`, `UserActive`, `LowImpact`, `Paused`. All four variants exist.
- **The policy** (`crates/pipeline/workers/src/scheduler/queue.rs:222`) —
  embedding is skipped under `UserActive`, citing RFC-036 §9.2 in the code. It
  is written and correct. It has simply never been reached.
- **The transitions** (`crates/pipeline/workers/src/scheduler/dispatch.rs:75,89`)
  — `notify_user_active()` and `notify_user_idle()`. Zero production callers.
- **`pause()` / `resume()`** — one caller each, at startup only, from
  `run_with_context`. `resume`'s catalog fix-up is unconditional as of `d9159b0`;
  do not re-gate it on `resource_mode` (see Review 175 §2 for why that guard was
  a bug).
- **The host loop** (`crates/app/src/scheduler_host.rs`) — spawned by RFC-056,
  currently sleeping on `IDLE_POLL` (300 ms) with no other wakeup.

**Your job is the wiring, not the mechanism.** If you find yourself writing a
new `ResourceMode` variant or a new skip rule in `queue.rs`, stop.

## 2. Slices, ordered by risk

### Slice 1 — the path, with one source

Build the channel (RFC-057 §4.1) and wire **user activity only** (§4.2).

This is deliberately the whole path with the cheapest source: no new dependency,
no settings, no platform differences, and it is the half that delivers value
immediately — today a user typing a query competes with embedding at ~144 ms per
document.

Producers send *observations*, not modes. The scheduler maps observation → mode
per RFC-036 §13. A producer that names `ResourceMode` directly has the coupling
backwards.

Drain the channel each loop iteration. Waking early on a signal is an
optimisation this slice does not need; if you do it, do it after the plain
version is green, and say so.

### Slice 2 — battery detection

Add the source behind an injection seam (§4.3, §6.1), and the §4.4 rename with
old-name-accepted-on-read.

**The dependency choice is yours to make and mine to review.** Record in the
review request: crate, version, maintenance status, platform coverage, licence,
and what it pulls in transitively. RFC-055 §2.3 is the precedent for recording
*why* a floor exists. Do not name a crate in the RFC — name it in the request,
where I can check it against the lockfile.

Thermal is out of scope, explicitly (§4.3). Do not add a `thermal` variant "for
later."

## 3. The two things most likely to go wrong

### 3.1 A test that proves the mode changed, not that anything happened

This is the defect class this programme has found nine times. `ResourceMode`
is an enum on a struct; asserting `scheduler.resource_mode() == UserActive`
after sending a signal is easy, passes, and proves nothing about whether
embedding actually deferred.

**Assert the scheduling consequence.** A signal arrives, and the next `tick()`
does not return an embedding job that it would otherwise have returned. That is
what `queue.rs:222` promises, and it is the only thing worth testing.

Then break it: revert `queue.rs:222`'s skip and watch your test fail. If it
still passes, it was testing the enum.

### 3.2 The `Paused` interaction

`notify_user_active` already guards against overriding `Paused`. A user typing a
query must not silently un-pause a scheduler the user turned off — that is
`background_indexing` reversibility (RFC-056 §9 criterion 4) regressing through a
new door.

The guard exists. **Prove it holds through the new path**, which is a different
call site than the one it was written for.

## 4. Testing — the same requirement as RFC-056

RFC-056 §8.8: every test exercises the shipped application's path. A test that
constructs a `Scheduler` and calls `notify_user_active()` by hand passes
identically whether or not the UI ever sends anything — which is exactly the
state the code is in today, and exactly how it stayed there.

**One honest exception, stated in RFC-057 §6.1:** battery state is not
controllable in CI, so the *detector* is tested at its seam. The *path* is not
exempt. Do not let the seam creep upward until the whole feature is tested one
layer down from the application.

RFC-057 §7's fourth criterion — real battery detection verified manually on one
machine — is a real criterion, not a formality. Report what machine, what state,
and what you observed. If you cannot run it, say so and leave it unticked; an
unticked criterion is fine, an inferred one is not.

## 5. Verification

- The usual workspace gates and three CI legs.
- The migration test (§6.2 item 5) against a **literal legacy `settings.json`
  file**, not a constructed struct — a struct with `#[serde(alias)]` tests the
  alias, not the file.
- Search latency must not regress. Slice 2 of RFC-056 measured 48.04 ms during
  indexing against 51.42 ms after; if user-activity signalling moves that, it is
  a finding.

## 6. Stop conditions

1. A source appears to require an RFC-036 policy change (§scope rule).
2. A signal can un-pause a scheduler the user paused (§3.2).
3. The channel starts carrying anything that is not a resource observation —
   job control belongs to the catalog per RFC-036 §16 (RFC-057 §8).
4. A `Path`/`PathBuf` would cross the spawn boundary — RFC-049 still applies.
5. An acceptance criterion cannot be exercised through the application's own
   path, other than the detector seam §4 names.

## 7. Not in scope

Memory pressure (RFC-036 §13.3); thermal detection; RFC-036 §14.3's Pause/Resume
UI control; a settings UI. §14.3 becomes one more producer once Slice 1 lands —
shape the channel so that stays true, but do not build it.
