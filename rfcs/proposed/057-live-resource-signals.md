# RFC-057: Live Resource Signals for the Indexing Scheduler

**Project:** orbok\
**RFC:** 057\
**Title:** Live Resource Signals for the Indexing Scheduler\
**Status:** Proposed\
**Target milestone:** indexing responsiveness\
**Date:** 2026-08-13\
**Related RFCs:** RFC-036 §13 Resource Awareness and §14.3 (this implements their signal sources; it does not re-specify their policy); RFC-056 Hosting the Indexing Scheduler (this depends on it and inherits one of its acceptance criteria); RFC-039 Privacy Modes

---

## 1. Summary

RFC-036's resource-awareness **policy is already implemented and has no signal
sources.** `ResourceMode` has four states, the scheduler already yields embedding
work when the mode says the user is active, and the transition methods exist —
but nothing in the application ever calls them, and one of the four states has
never been set by any code at all.

This RFC delivers signals into the running scheduler: a live path into the task
RFC-056 spawned, plus the two sources RFC-036 §13.1 and §13.2 name.

It does not change any scheduling policy. Every decision about *what* to do in
each mode is RFC-036's and stays as written.

## 2. Triggering evidence

### 2.1 The consumer is built

`crates/pipeline/workers/src/scheduler/`:

- `ResourceMode` — `Normal`, `UserActive`, **`LowImpact`** (documented as
  "battery/thermal policy"), `Paused`.
- `queue.rs:222` — the policy, with RFC-036 cited in the code:

  ```rust
  if q.kind() == QueueKind::Embedding && resource_mode == ResourceMode::UserActive {
      continue; // RFC-036 §9.2: yield embedding to active search.
  }
  ```

- `dispatch.rs` — `notify_user_active()`, `notify_user_idle()`, `pause()`,
  `resume()`.

### 2.2 Nothing drives it

| Transition | Production callers |
|---|---:|
| `notify_user_active()` | **0** |
| `notify_user_idle()` | **0** |
| `ResourceMode::LowImpact` set anywhere | **0** |
| `pause()` / `resume()` | 1 each — **at startup only** |

So embedding never yields to an active search, and the low-impact state is
unreachable. The scheduler runs permanently in `Normal` except for a
startup-time pause.

### 2.3 There is no live path into the running task

RFC-056 Slice 3 hit this directly and correctly declined to build it:
`background_indexing` is read **once at startup** because no channel exists from
the UI into the spawned scheduler task. `scheduler_host`'s own comment records
the same about job enqueueing — *"it does not wake this task, so idle periods are
bridged by polling."*

The loop today sleeps on a 300 ms `IDLE_POLL` and has no other wakeup.

### 2.4 One inherited criterion, and a naming problem

RFC-056 §9's fifth criterion — *"With `pause_on_battery` on and the machine on
battery, indexing pauses"* — was unmet at that RFC's completion, because
RFC-036 §13.2 explicitly permits deferring battery detection. **That criterion
moves here** rather than being dropped.

It also overstates. RFC-036 §13.2 says:

```text
on battery      → reduce background work
low battery     → pause heavy work
thermal warning → pause embedding
```

*Reduce*, not pause, until the battery is low. The setting is named
`pause_on_battery` and the criterion said "pauses"; both describe a binary where
the policy is graduated. §4.4 settles this.

### 2.5 The setting is currently unreachable

`pause_on_battery` exists only as a field in `settings.json`. There is no
settings UI. This is why implementing detection alone would have served nobody,
and why §4.2's user-activity source — which needs no settings at all — is the
higher-value half.

## 3. Decision

Add **one signal path** into the scheduler task, and feed it from the sources
RFC-036 §13 names. The path is general: sources are added to it, not alongside
it.

## 4. Required behaviour

### 4.1 The path

A single channel the scheduler loop drains each iteration, carrying resource
signals from any producer. It must not require a producer to know about the
scheduler's internals — producers report *observations* (the user typed; the
machine is on battery), and the scheduler maps observations to `ResourceMode`
per RFC-036 §13.

The loop currently wakes on a 300 ms poll. Draining on each iteration is
sufficient and keeps the change small; waking early on a signal is an
optimisation, not a requirement.

### 4.2 Source: user activity (RFC-036 §13.1)

Highest value, no settings required, works on every machine. Search input and
submission are the signals; `notify_user_active()` and `notify_user_idle()` are
the existing sinks.

**This is the source that makes `queue.rs:222`'s already-written policy do
something.** Today a user typing a query competes with embedding at ~144 ms per
document.

### 4.3 Source: battery and thermal (RFC-036 §13.2)

Maps to `ResourceMode::LowImpact`, the variant nothing has ever set.

Detection needs a cross-platform source, which orbok does not currently have.
Selecting one is a dependency decision to make explicitly rather than
incidentally — RFC-055 §2.3 is the precedent for recording *why* a floor exists.
This RFC deliberately names no crate or version. The implementer evaluates
candidates and records maintenance status, platform coverage, and licence in the
review request, where the claim can be checked against the lockfile.

**Thermal is out of scope**, and named as such: §13.2 lists it, no portable
detection exists, and pretending otherwise would repeat the pattern of a
criterion nothing can satisfy.

### 4.4 What "on battery" does

Per RFC-036 §13.2, **on battery reduces; it does not pause.** Low battery pauses
heavy work.

The setting's name must follow. `pause_on_battery` describes a binary the policy
does not have.

**This RFC's position:** the field is renamed to match the graduated policy, and
the old name is accepted on read so existing `settings.json` files keep working.
Renaming a persisted field is a migration cost, stated here rather than
discovered later (§8). The alternative — keep the name, implement the graduated
behaviour — leaves a setting whose name misdescribes it, which is precisely the
drift that produced RFC-056 §9's overstated criterion.

This is a decision the owner may overturn; it is written as a position rather
than an open question so that accepting the RFC settles it.

### 4.5 Not required: the pause/resume control

RFC-036 §14.3's live Pause/Resume UI becomes trivial once §4.1 exists — it is one
more producer. It needs a settings surface that does not exist, so it is out of
scope, but **the channel must not be shaped so that adding it later is awkward.**

## 5. Non-goals

1. **Any change to RFC-036's policy.** What each mode does is settled. If
   implementation suggests a policy is wrong, that is an RFC-036 amendment and a
   separate conversation.
2. **Memory pressure (RFC-036 §13.3).** A third source, deliberately deferred:
   choosing thresholds is its own measurement problem, and a wrong threshold
   pauses indexing on a healthy machine. The channel is general, so adding it
   later is additive.
3. **A settings UI.**
4. **Thermal detection** — §4.3.

## 6. Testing requirements

### 6.1 The honest tension, stated

RFC-056 §9's lesson was that criteria must name observable behaviour of the
running application. **Battery state is not controllable in CI**, so a criterion
like "on battery, indexing reduces" cannot be exercised there.

The resolution is to split the claim, not to weaken it:

- **The signal path** is testable end-to-end: inject a signal, observe the mode
  change and the scheduling consequence. That is real application behaviour.
- **The detector** is testable only at an injection seam — the same shape as
  `PlatformRuntimePaths` (RFC-049) and upstream `app-json-settings`'
  `config_dir_from`. Its correctness against real hardware is a manual check,
  and should be recorded as such rather than implied.

A criterion that cannot be reached is worse than one honestly scoped. Do not
write "on battery" into a criterion the suite cannot satisfy.

### 6.2 Specific

1. A user-activity signal reaches the scheduler and embedding **actually defers**
   — assert the scheduling consequence via `queue.rs:222`, not merely that the
   mode changed.
2. Idle restores `Normal` and embedding resumes.
3. `Paused` is not overridden by a user-activity signal — `notify_user_active`
   already guards this; prove the guard holds through the new path.
4. Battery detection at its seam, both states, no real battery required.
5. A `settings.json` written with the **old** field name still loads and still
   honours the preference — the §4.4 migration, tested against a literal legacy
   file rather than a constructed struct.
6. The three CI legs.

## 7. Acceptance criteria

- [ ] With a search in progress, embedding work defers, and resumes when the
      search ends — observed through the scheduler, not asserted on the mode
      field alone.
- [ ] A signal delivered while the scheduler is `Paused` does not un-pause it.
- [ ] With the battery source injected as "on battery", the scheduler enters
      `LowImpact` and background work reduces per RFC-036 §13.2.
- [ ] Real battery detection is verified manually on at least one machine, and
      the result recorded — not inferred from the injected test.
- [ ] A profile whose `settings.json` predates the §4.4 rename keeps its
      preference across an upgrade.
- [ ] Nothing regresses with no signals present: the scheduler behaves exactly as
      it does today.

## 8. Risks

**The channel becomes a general command bus.** The temptation once a path exists
is to send everything through it. It carries resource observations; job control
belongs to the catalog, which RFC-036 §16 already makes the source of truth.

**A rename with a migration.** §4.4's naming decision touches persisted user
settings. RFC-055 §7 is the precedent for stating the cost plainly rather than
discovering it.

## 9. Note to the reviewer

Every claim in §2 was read from the current tree: the four `ResourceMode`
variants, `queue.rs:222`'s policy and its RFC citation, the zero call counts, and
`LowImpact` never being set.

The framing changed during drafting. This began as "wire `pause_on_battery`,"
which RFC-056 §9 required and RFC-036 §13.2 permits deferring. Investigating
showed battery is one of three missing sources — plus a UI control — for a policy
already written, and that scoping an RFC to battery alone would build a
single-purpose path needing widening every time another source arrived. The owner asked whether it could be an independent theme; it can, but
not at the size the question started from.
