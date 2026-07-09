# RFC-046: Declared Candle Embedding Backend — Status and Options

**Project:** orbok  
**Former project name:** orbit  
**RFC:** 046  
**Title:** Declared Candle Embedding Backend — Status and Options  
**Status:** Implemented (v0.22.0)  
**Target milestone:** v0.22.0 stabilization  
**Date:** 2026-06-21  
**Decision revision:** 2026-06-30 — following external review, Option **B1** selected. The option analysis (§6) is preserved as drafted; the decision is recorded in §10.  
**Related RFC:** RFC-021 Default Embedding Model Selection  

---

## 0. Nature and history of this RFC

This is a **decision RFC**. It records a factual situation in
`orbok-embed`, lays out the options for resolving it, and — as of the
decision revision — records the selected direction.

**This RFC was drafted strictly neutrally**: §6 presents the options with
no recommendation, and the option lettering and ordering carry no ranking.
That neutral analysis is preserved unchanged. Following external review
(2026-06-30, Rust architecture / release-gating / RFC-lifecycle), Option
**B1** was selected. The decision and its rationale are recorded in §10;
the implementation contract is in §11–§12. An implementation handoff
(`HANDOFF-046`) accompanies this RFC.

The neutral drafting and the subsequent decision are not in tension: the
options were laid out without prejudice so the review could deliberate
freely, and the review then reached a direction. This is the normal
RFC-000 lifecycle (Proposed → reviewed → Accepted).

---

## 1. Subject

`crates/search/embed/src/lib.rs` declares an embedding backend named
**Candle**, gated behind a Cargo feature named `candle`. The module file
that the declaration refers to, `candle_backend.rs`, is not present in the
source tree, and — per the evidence in §3 — has never been present in any
released version of the project.

This RFC concerns what, if anything, to do about that.

---

## 2. The declaration as it stands

`crates/search/embed/src/lib.rs` contains, gated by the feature:

```rust
#[cfg(feature = "candle")]
mod candle_backend;
```

and a matching dispatch arm in `create_embedding_model()`:

```rust
InferenceBackend::CandleCpu | InferenceBackend::CandleCuda => {
    #[cfg(feature = "candle")]
    {
        candle_backend::create(config)
    }
    #[cfg(not(feature = "candle"))]
    {
        Err(OrbokError::Cache(
            "Candle inference is not compiled in. \
             Rebuild with: --features orbok-embed/candle"
                .into(),
        ))
    }
}
```

`crates/search/embed/Cargo.toml` declares the feature and two optional
dependencies that it enables:

```toml
[features]
candle = ["dep:candle-core", "dep:candle-nn"]

[dependencies]
candle-core = { version = "0.10", optional = true }
candle-nn   = { version = "0.10", optional = true }
```

The crate's doc-comment backend table also lists a `Candle` row.

The sibling backend `tract` (the `OnnxRuntime` path) is declared the same
way and **does** have its backing file, `tract_backend.rs`.

---

## 3. Evidence and established facts

The following were checked directly against source artifacts.

1. **The file is absent from the current tree.** No
   `crates/search/embed/src/candle_backend.rs` exists at v0.21.0.

2. **The file is absent from earlier releases.** The v0.9.5 release
   archive (pre directory-restructure) contains only `lib.rs` and
   `tract_backend.rs` under the embed crate's `src/`. Releases across
   v0.2–v0.8 were also checked and contain no `candle_backend.rs`. No
   released version has ever contained the file.

3. **RFC-021 is a model-selection RFC, not a backend-implementation RFC.**
   RFC-021 (`Implemented (v0.7.0)`) selects a default embedding *model*
   (`multilingual-e5-small`). It references "candle" once, as a candidate
   *evaluation criterion* ("Runtime support: candle/ONNX/local backend
   feasibility"). It does not scope a Candle backend implementation as a
   deliverable.

4. **The CHANGELOG is internally consistent with the above.** The v0.7.0
   entry describes the embedding factory's three-arm dispatch design (Mock
   always; OnnxRuntime under `tract`; Candle under `candle`) and the
   feature-flag error fallback. Its test line records four `orbok-embed`
   tests — mock backend, feature-flag error, and defaults — i.e. no Candle
   test. A later entry records a `candle-core`/`candle-nn` 0.9→0.10
   optional-dependency version bump; this is a manifest change and does not
   imply the module file existed.

5. **A "lost file" hypothesis was considered and ruled out.** The
   possibility that `candle_backend.rs` once existed and was dropped during
   packaging was examined. The v0.9.5 archive evidence in (2) rules it out:
   the file was never shipped, so there is nothing that was lost.

**Resulting factual position:** the Candle backend is a declared dispatch
target whose implementation file has never existed. The declaration has
been carried, unchanged in substance, since the `orbok-embed` factory was
introduced (v0.7.0).

---

## 4. Current observable behavior

Stated without judgement:

- **Default builds are unaffected.** Without `--features candle`, the
  `#[cfg(not(feature = "candle"))]` arm is selected, the `candle_backend`
  module is never referenced, and the crate compiles. `cargo build` and
  `cargo test --workspace --lib` succeed. The full suite passes (387 tests
  at v0.21.0).

- **The embedding backend in actual use** is the `tract` ONNX path (when
  built with `--features tract` and a model is configured) or the
  always-available `Mock` backend. Candle is not part of any shipped build.

- **`--features candle` does not compile.** Enabling the feature activates
  the `#[cfg(feature = "candle")] mod candle_backend;` declaration, which
  fails module resolution because the file is absent.

A separate tooling fact is recorded in §7, kept apart from the behavioral
description here.

---

## 5. Scope of the decision

In scope:

- Whether the Candle backend declaration in `orbok-embed` is retained,
  removed, or left pending a later decision.
- The handling of the `candle` feature and the optional `candle-core` /
  `candle-nn` dependencies.
- The handling of the `candle_backend` module declaration and its dispatch
  arm.
- The handling of the `InferenceBackend::CandleCpu` /
  `InferenceBackend::CandleCuda` enum variants in `orbok-models`.
- Any documentation that references Candle (the `orbok-embed` doc table;
  RFC-021's criterion mention).

Out of scope:

- The `tract` / `OnnxRuntime` backend, which is implemented and present.
- The `Mock` backend.
- The choice of default embedding model (settled by RFC-021).

---

## 6. Option space

The options below are grouped into three families: **A — retain**, **B —
remove**, **C — defer**. Each family lists sub-variants. The structure of
each entry is identical so the options can be compared on equal terms.

Lettering and order imply no preference.

### Option A — Retain the Candle declaration and make `--features candle` resolvable

**Intent.** Keep Candle as a declared backend and bring the feature build
to a resolvable state.

Sub-variants:

- **A1 — Stub file.** Add `candle_backend.rs` containing a `create()` that
  matches the backend interface and returns an error indicating the backend
  is not implemented, gated by the `candle` feature.
- **A2 — Restructure the gating.** Keep the feature and dependencies
  declared, but arrange the `mod`/dispatch so no reference to a missing
  module exists until a file is present (for example, routing the feature
  arm to the same error path used when the feature is off).
- **A3 — Implement the backend.** Provide a working Candle backend
  (`candle-core`/`candle-nn`), making the feature functional.

**Effect on `--features candle`.** A1/A2: compiles, returns an error at
runtime (A1) or behaves as the not-compiled-in path (A2). A3: compiles and
performs inference.

**Effect on the public API (`orbok-models::InferenceBackend`).** No change;
the `CandleCpu`/`CandleCuda` variants remain backed by a dispatch arm.

**Effect on documentation.** No removal needed; RFC-021's mention and the
doc table remain consistent. A3 would add usage documentation.

**Factors a reviewer may weigh.**

- Preserves Candle as a declared/available backend slot.
- Keeps the `candle-core`/`candle-nn` optional dependencies in the manifest
  (compiled only under the feature).
- A1/A2 leave a feature that builds but does not perform inference; the
  declared capability and the actual capability differ until A3 is done.
- A3 entails implementing and maintaining a second inference backend.

### Option B — Remove the Candle declaration

**Intent.** Remove the declared Candle backend from `orbok-embed`.

Sub-variants:

- **B1 — Remove backend wiring, keep enum variants.** Remove the
  `#[cfg(feature = "candle")] mod candle_backend;` line, the `candle`
  feature, the optional `candle-core`/`candle-nn` dependencies, and the
  doc-table row; collapse the `CandleCpu | CandleCuda` dispatch arm to the
  not-available error unconditionally. Keep
  `InferenceBackend::CandleCpu`/`CandleCuda` as variants that route to that
  error.
- **B2 — Remove backend wiring and enum variants.** As B1, and additionally
  remove the `CandleCpu`/`CandleCuda` variants from
  `orbok-models::InferenceBackend`.

**Effect on `--features candle`.** The feature no longer exists; the
build-resolution condition is removed.

**Effect on the public API (`orbok-models::InferenceBackend`).** B1: no
change to the enum (variants remain, routed to error). B2: removes two
variants from a public enum; any exhaustive match on `InferenceBackend`
(internal or external) would require updating.

**Effect on documentation.** Removes the Candle row from the `orbok-embed`
doc table. RFC-021's one-line criterion mention would be inconsistent with
the new state unless RFC-021 is amended or annotated (see §8).

**Factors a reviewer may weigh.**

- Removes a declared capability that has never been implemented.
- Drops the `candle-core`/`candle-nn` optional dependencies from the
  manifest.
- B1 keeps the model-layer enum surface stable; B2 changes it.
- Reintroducing Candle later would require re-adding the feature,
  dependencies, and wiring (and, for B2, the variants).

### Option C — Defer the substantive decision

**Intent.** Take no position now on retain-vs-remove; revisit later.

Sub-variants:

- **C1 — Defer with no source change.** Leave the declaration as-is and
  carry this RFC in `proposed/` until a decision is made.
- **C2 — Defer, bound to a future RFC.** Leave the declaration as-is and
  record that the Candle question is to be resolved as part of a future
  embedding-backend RFC.

**Effect on `--features candle`.** Unchanged (does not resolve).

**Effect on the public API.** Unchanged.

**Effect on documentation.** Unchanged.

**Factors a reviewer may weigh.**

- Makes no commitment while the timing of any Candle work is unknown.
- Leaves the present state (and the §7 tooling fact) in place for now.
- Postpones rather than resolves the question.

---

## 7. Tooling fact (recorded, not weighted)

This section records one additional fact for completeness. It is stated
neutrally and is **not** presented as a reason to prefer any option.

- Workspace-level `cargo fmt` aborts with a module-resolution error on the
  absent `candle_backend.rs`, because `rustfmt` walks `mod` declarations
  regardless of `#[cfg]`. Formatting individual files with `rustfmt`
  directly is unaffected.

Whether, and how, this interacts with any option is left entirely to the
reviewer's deliberation. No mechanics for addressing it are asserted here;
they would be settled during implementation of whichever direction is
chosen.

---

## 8. Cross-cutting note: RFC-021

RFC-021 mentions "candle" once, as a backend-feasibility evaluation
criterion (§3 item 3). This is recorded here as a fact a reviewer may wish
to account for: options that change Candle's status (notably Option B) would
leave that mention describing a backend that is no longer declared, which a
reviewer may consider addressing via an annotation or amendment to RFC-021.
This note implies no preference among the options.

---

## 9. Open questions — resolved by the decision

These were the open questions in the neutral draft. The decision (§10)
resolves them as follows:

1. **Family selected:** B — remove.
2. **(Retain sub-variant):** n/a.
3. **Enum variants:** kept (B1). `InferenceBackend::CandleCpu`/`CandleCuda`
   remain in `orbok-models`, routed to a stable not-supported error. A
   future deprecation/removal may be considered in a later backend-API RFC.
4. **§7 tooling fact:** addressed — removing the `mod candle_backend;`
   declaration removes the missing-module path, so workspace `cargo fmt`
   resolves. A `cargo fmt --check` gate is added (§11) to prevent recurrence.
5. **RFC-021 reconciliation (§8):** RFC-021 is **annotated** (not rewritten)
   to clarify that Candle was a feasibility *criterion* during model
   selection, not implemented support.
6. **Release:** v0.22.0 stabilization.

---

## 10. Decision

**Selected option: B1 — Remove backend wiring, keep enum variants.**

The current Candle backend declaration is removed from `orbok-embed`
because the backing module (`candle_backend.rs`) has never existed in any
release, no accepted RFC scoped Candle implementation as a deliverable, and
the declared `candle` feature does not compile.

Specifically:

- The `#[cfg(feature = "candle")] mod candle_backend;` declaration and the
  feature-gated `candle_backend::create(config)` dispatch are removed.
- The `candle` Cargo feature and the optional `candle-core` / `candle-nn`
  dependencies are removed from `orbok-embed`.
- The `CandleCpu | CandleCuda` dispatch arm is collapsed to an
  **unconditional** not-supported error.
- `InferenceBackend::CandleCpu` and `InferenceBackend::CandleCuda` remain in
  `orbok-models` for model-layer/API stability, but selecting either returns
  a stable error. The error message no longer instructs the user to rebuild
  with a (now non-existent) feature.
- The `orbok-embed` backend documentation table is updated to remove Candle
  as an available backend.
- RFC-021 is annotated to clarify Candle's status as a feasibility
  criterion, not implemented support.

A future RFC may reintroduce Candle if and when there is a concrete
implementation plan, CI coverage, and a backend-parity test strategy.

### 10.1. Error message contract

After B1, the not-supported error for the Candle variants is stable and
honest:

```text
Candle inference is not currently supported. Use the ONNX backend.
```

It must **not** reference rebuilding with `--features orbok-embed/candle`,
since that feature no longer exists.

---

## 11. Acceptance criteria

- Workspace `cargo fmt --check` succeeds (no missing-module path remains).
- `cargo test --workspace --lib` passes.
- `cargo check -p orbok-embed` succeeds (default build).
- The default build and the `Mock` backend are unaffected; the `tract`
  backend wiring is untouched by this change. (Note: `--features tract` has a
  pre-existing build issue unrelated to candle, tracked separately in
  `rfcs/appendices/FINDING-tract-feature-build.md`; it is out of RFC-046's
  scope.)
- No `candle` feature is advertised in `orbok-embed/Cargo.toml`.
- Selecting `InferenceBackend::CandleCpu`/`CandleCuda` returns the §10.1
  error.
- The `orbok-embed` backend doc table no longer lists Candle.
- RFC-021 is annotated (not rewritten historically).
- A future Candle backend can be reintroduced by a new RFC.

---

## 12. Feature-contract principle

This decision is grounded in a release-engineering principle the project
adopts going forward:

> A declared Cargo feature must compile, or it must not exist. No declared
> feature may fail because of a missing module.

A workspace feature-resolution check is added so a declared feature can no
longer reference an absent module without CI catching it.
