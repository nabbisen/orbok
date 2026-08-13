# RFC-057: Live Resource Signals for the Indexing Scheduler

**Project:** orbok\
**RFC:** 057\
**Title:** Live Resource Signals for the Indexing Scheduler\
**Status:** Accepted\
**Target milestone:** indexing responsiveness\
**Date:** 2026-08-13\
**Accepted:** 2026-08-13 by the project owner\
**Handoff:** [`HANDOFF-057-live-resource-signals.md`](../handoffs/HANDOFF-057-live-resource-signals.md)\
**Related RFCs:** RFC-036 §13 Resource Awareness and §14.3 (this implements their signal sources; it does not re-specify their policy); RFC-056 Hosting the Indexing Scheduler (this depends on it and inherits one of its acceptance criteria); RFC-039 Privacy Modes

---

## 1. Summary

RFC-036's resource-awareness policy for **user activity** is implemented and has
no signal source. `ResourceMode` has four states, the scheduler already yields
embedding work when the mode says the user is active, and the transition methods
exist — but nothing in the application ever calls them.

This RFC delivers signals into the running scheduler: a live path into the task
RFC-056 spawned, plus the two sources RFC-036 §13.1 and §13.2 name.

> **Amendment 1 (2026-08-13), after Slice 1 landed.** This summary originally
> read *"policy is already implemented and has no signal sources"* — of all four
> states. That is true of `UserActive` and **false of `LowImpact`**, which has no
> policy at all, only an enum variant (§2.6). The correction changes what Slice 2
> must build and resolves a contradiction the original §5.1 created. Every
> amended section is marked. Slice 1 is unaffected — it shipped against the half
> that was correct.

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

**This is the whole of it.** `queue.rs:222` is the only line in the scheduler
that reads `resource_mode` for anything other than `Paused`, and it tests
`UserActive` alone.

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

### 2.6 `LowImpact` has no policy, only a name (Amendment 1)

Searching the tree for `LowImpact` returns exactly two hits:

- `crates/pipeline/workers/src/scheduler/job.rs:207` — the enum variant.
- `crates/pipeline/workers/src/scheduler.rs:14` — a doc comment listing it.

No setter, no consumer, no test. Setting a mode nothing reads changes nothing —
it would be `notify_user_active` again, a transition with no consumer, which is
the exact defect this RFC exists to close.

**Slice 2 must therefore write RFC-036 §13.2's policy as well as its source.**
That is implementing an intent RFC-036 states and never implemented, not
changing RFC-036's decisions; §5.1 is narrowed accordingly.

Found by Review 177 §4 while reviewing Slice 1, in the RFC written to fix the
same class of error in RFC-056 §9.

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

**Amended by Amendment 1.** Maps to `ResourceMode::LowImpact` **and gives that
variant its first policy** (§2.6).

#### 4.3a What "reduce background work" concretely means

RFC-036 §13.2 says *reduce*. The obvious reading — lower the concurrency limits —
**is not available**: `SchedulerLimits::default()` sets every worker count to
`1`, and 1 cannot be reduced. Anything below it is a pause, not a reduction.

The lever that does exist is the queue mix. Per RFC-048's measurement,
`document_embedding_ms` is **99.93%** of indexing cost — 143.9 ms per document
against ~1.3 ms for extract, chunk and keyword together. So:

> **`LowImpact` skips the embedding queue and lets everything else run.**

That is a ~99.9% reduction in background work while indexing genuinely continues:
files still become searchable by keyword, they simply do not gain vectors until
the machine is back on mains. It is the same mechanism `UserActive` already uses
at `queue.rs:222`, for a different reason — a one-line change, which is the
correct size for implementing a policy RFC-036 already decided.

#### 4.3b Low battery is deferred, because it would be unobservable

RFC-036 §13.2 distinguishes *on battery → reduce* from *low battery → pause heavy
work*. With today's queues those collapse: the only heavy work is embedding, and
§4.3a already stops it on battery. A low-battery state would be behaviourally
identical to `LowImpact`, so its acceptance criterion could not be written as
observable behaviour.

**Deferred until there is a second class of heavy work to distinguish.** This
also removes the need to pick a battery percentage threshold — the least
defensible number in the original draft.

#### 4.3c The mode is derived, never mutated per source

The original draft had each source call a transition. With one source that is
correct, and it is what Slice 1 shipped. With two it silently loses state:

- battery source → `LowImpact`
- user types → `notify_user_active` overwrites anything not `Paused` →
  `UserActive`
- user stops → `notify_user_idle` returns to **`Normal`**, not `LowImpact`

Battery awareness evaporates after the first search and does not return until
the source signals again — potentially hours, if it signals on plug/unplug.

**Slice 2 changes the host loop to hold observation state and derive the mode**,
rather than having each source mutate it:

```text
Paused       if background_indexing is off      (a user command, not an observation)
UserActive   else if user active within USER_IDLE_TIMEOUT
LowImpact    else if on battery
Normal       otherwise
```

Derivation is what fixes the defect: `LowImpact` is recomputed every iteration,
so it cannot be lost — it returns by itself the moment the user stops typing.

**On the `UserActive` / `LowImpact` precedence.** Today the two produce the
identical restriction (§4.3a), so the ordering is unobservable and the rule
exists only to be defined before it matters. When they diverge, the rule is
**stricter-wins**, and if a future pair is not orderable — one restricting work
the other permits — that is the signal to stop collapsing conditions into one
enum and raise it as an RFC-036 amendment. Do not quietly widen the precedence
list instead.

`Paused` stays outside the derivation: it is a persisted user command with
catalog state (RFC-036 §16), not an observation, and `resume()`'s fix-up must
keep working exactly as Review 175 settled it.

#### 4.3d Detection itself

Detection needs a cross-platform source, which orbok does not currently have.
Selecting one is a dependency decision to make explicitly rather than
incidentally — RFC-055 §2.3 is the precedent for recording *why* a floor exists.
This RFC deliberately names no crate or version. The implementer evaluates
candidates and records maintenance status, platform coverage, and licence in the
review request, where the claim can be checked against the lockfile.

**Thermal is out of scope**, and named as such: §13.2 lists it, no portable
detection exists, and pretending otherwise would repeat the pattern of a
criterion nothing can satisfy.

Detection comes **last** in the slice: §4.3a and §4.3c are both fully testable
against an injected source, and they are the parts that can be got wrong.

### 4.4 The setting's name

**The rename stands; its justification is sharper than when the owner accepted
it, and the change is worth stating rather than absorbing silently.**

The original argument was that §13.2 says *reduce* where the field says *pause*.
§4.3a now makes "reduce" concrete — embedding stops, everything else continues —
so the field does pause something. The name is not wrong so much as **silent
about its own scope**: a user reading `pause_on_battery` reasonably expects
indexing to stop on battery, and it will not. Files keep being scanned,
extracted, chunked and made keyword-searchable; only vectors wait.

That gap between what the name promises and what happens is the same defect
class as a capability-phrased acceptance criterion — technically defensible,
and it misleads. So: **renamed to say what it pauses**, with the old name
accepted on read so existing `settings.json` files keep working (§8's migration
cost).

The owner accepted the rename on the original reasoning. The conclusion is
unchanged; if the revised reasoning changes their view, this is the place to
say so.

### 4.5 Not required: the pause/resume control

RFC-036 §14.3's live Pause/Resume UI becomes trivial once §4.1 exists — it is one
more producer. It needs a settings surface that does not exist, so it is out of
scope, but **the channel must not be shaped so that adding it later is awkward.**

## 5. Non-goals

1. **Any change to RFC-036's *decisions*.** *(Narrowed by Amendment 1 — as
   originally written, this forbade the work §4.3 requires.)* `UserActive`'s and
   `Paused`'s behaviour is settled and must not move. Writing `LowImpact`'s
   policy is **in scope**: RFC-036 §13.2 decided what should happen and no code
   implements it (§2.6), so §4.3a implements RFC-036's stated intent rather than
   substituting orbok's judgement for it. Inventing policy RFC-036 did *not*
   state remains out of scope, and if implementation suggests one of its
   decisions is wrong, that is still an RFC-036 amendment and a separate
   conversation.
2. **Low-battery as a distinct state** — §4.3b, deferred because it would be
   behaviourally identical to `LowImpact` and so could not be given an observable
   criterion.
3. **Memory pressure (RFC-036 §13.3).** A third source, deliberately deferred:
   choosing thresholds is its own measurement problem, and a wrong threshold
   pauses indexing on a healthy machine. The channel is general, so adding it
   later is additive.
4. **A settings UI.**
5. **Thermal detection** — §4.3.

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
6. **(Amendment 1)** The derivation, §4.3c, tested at the interleaving that
   motivated it: on battery → user types → user stops → **still on battery**.
   Assert the scheduling consequence, not the mode field. This is the test the
   per-source-mutation design fails; if it passes before the derivation lands,
   it is testing the wrong thing.
7. **(Amendment 1)** `LowImpact` skips embedding and lets extract/chunk/keyword
   run — both halves. A test asserting only that embedding stopped would pass
   against a mode that stopped everything, which is §4.3a's whole distinction.
8. The three CI legs.

## 7. Acceptance criteria

**Slice 1 — met** (Reviews 177, 178; commits `98fbfd3`, `7a7b728`):

- [x] With a search in progress, embedding work defers, and resumes when the
      search ends — observed through the scheduler, not asserted on the mode
      field alone.
- [x] A signal delivered while the scheduler is `Paused` does not un-pause it.
- [x] Nothing regresses with no signals present: the scheduler behaves exactly as
      it does today.

**Slice 2 — code complete** (Review 179; commit `1d2b234`), **one criterion
blocked on hardware:**

- [x] With the battery source injected as "on battery", embedding stops **and
      extract/chunk/keyword keep running** — §4.3a's reduction, both halves.
      Verified complementary: removing the skip fails the first half only,
      stopping all work fails the second half only.
- [x] On battery, through a search and out the other side, the machine is still
      treated as on battery — §4.3c's derivation, asserted on the scheduling
      consequence.
- [x] A profile whose `settings.json` predates the §4.4 rename keeps its
      preference across an upgrade — against literal legacy JSON, not a
      round-tripped struct.
- [ ] **Real battery detection is verified manually on at least one machine, and
      the result recorded — not inferred from the injected test.** The
      development host has no system battery: `/sys/class/power_supply/` contains
      only `hidpp_battery_0` (`scope: Device`, a wireless mouse). The real
      detector was exercised there and correctly returns `None` — the
      no-battery-present branch, which is a real data point and *not* this
      criterion. **Open, awaiting hardware with a battery.** This is the only
      thing standing between RFC-057 and `done/`.

## 8. Risks

**The channel becomes a general command bus.** The temptation once a path exists
is to send everything through it. It carries resource observations; job control
belongs to the catalog, which RFC-036 §16 already makes the source of truth.

**A rename with a migration.** §4.4's naming decision touches persisted user
settings. RFC-055 §7 is the precedent for stating the cost plainly rather than
discovering it.

**The derivation quietly becoming a precedence list.** §4.3c defines an ordering
for two conditions that currently do the same thing. The failure mode is a third
and fourth condition being slotted into that list one at a time, each defensible
alone, until the enum encodes a policy nobody decided. The stop condition is in
§4.3c and it is deliberately sharp: conditions that are not orderable go to
RFC-036, not into the list.

## 9. Note to the reviewer

Every claim in §2 was read from the current tree: the four `ResourceMode`
variants, `queue.rs:222`'s policy and its RFC citation, the zero call counts, and
`LowImpact` never being set.

**Amendment 1 correction (2026-08-13).** §2.1's original claim that the policy
was implemented was checked by reading `queue.rs:222` and confirming a policy
existed — then generalised from that one variant to all four without checking the
others. `LowImpact` had no policy at all. This is the same error as RFC-056 §9's
criterion 5, committed in the RFC written to correct it: verifying one instance
of a claim and asserting the class. Caught by Review 177 §4 only because Slice 1's
shape made the second source's requirements concrete.

The framing changed during drafting. This began as "wire `pause_on_battery`,"
which RFC-056 §9 required and RFC-036 §13.2 permits deferring. Investigating
showed battery is one of three missing sources — plus a UI control — for a policy
already written, and that scoping an RFC to battery alone would build a
single-purpose path needing widening every time another source arrived. The owner asked whether it could be an independent theme; it can, but
not at the size the question started from.
