# Implementation Handoff — RFC-046: Declared Candle Embedding Backend (Option B1)

**Project:** orbok  
**RFC:** 046  
**Decision:** B1 — remove Candle backend wiring; keep `InferenceBackend`
variants routed to a stable not-supported error.  
**Target release:** v0.22.0 (stabilization)  
**Primary owner:** search/embed + model layer + release tooling

> **Scope rule:** This is a stabilization cleanup, not a feature. It removes
> a declared-but-never-implemented backend whose missing module breaks
> `--features candle` and workspace `cargo fmt`. It does **not** implement
> Candle and does **not** change shipped default behavior.

## 1. Outcome

After this change:

- `orbok-embed` no longer declares a `candle` feature, optional
  `candle-core`/`candle-nn` dependencies, or a `candle_backend` module.
- `InferenceBackend::CandleCpu`/`CandleCuda` remain public in `orbok-models`
  but route to a stable not-supported error.
- Workspace `cargo fmt` resolves (no missing-module path).
- `tract` (ONNX) and `Mock` backends are unchanged.

## 2. PR plan

### PR 1 — Remove Candle feature wiring (`orbok-embed`)

`crates/search/embed/src/lib.rs`:

- Remove `#[cfg(feature = "candle")] mod candle_backend;`.
- Collapse the `CandleCpu | CandleCuda` dispatch arm to an **unconditional**
  error (no `#[cfg]` split, no `candle_backend::create` call):

  ```rust
  InferenceBackend::CandleCpu | InferenceBackend::CandleCuda => Err(
      OrbokError::Cache(
          "Candle inference is not currently supported. Use the ONNX backend."
              .into(),
      ),
  ),
  ```

- Remove the Candle row from the crate doc-comment backend table; adjust the
  prose that names the `candle` feature.

`crates/search/embed/Cargo.toml`:

- Remove the `candle` feature.
- Remove the optional `candle-core` and `candle-nn` dependencies.
- Update the package `description` if it names candle.

**Acceptance:** `cargo check -p orbok-embed` passes; no `candle` feature
exists; default build unaffected. The separate `tract` feature check remains
out of scope for RFC-046 and is tracked in
`rfcs/appendices/FINDING-tract-feature-build.md`.

### PR 2 — Docs and RFC-021 annotation

- `orbok-models`: add a tracking note on the `CandleCpu`/`CandleCuda`
  variants (a doc comment, **not** `#[deprecated]`) pointing to RFC-046 and
  noting a future backend-API RFC may revisit them.
- Annotate RFC-021 (`rfcs/done/021-...`) — a short note that Candle was a
  feasibility *criterion*, never an implemented backend; resolved by
  RFC-046. **Annotate, do not rewrite** the historical text.
- CHANGELOG `[0.22.0]` entry.
- Check README / developer docs / model-config docs for any Candle mention
  and update.

**Acceptance:** docs no longer present Candle as available; RFC-021 history
preserved with annotation.

### PR 3 — Gates

- Restore/confirm a workspace `cargo fmt --check` gate.
- Add a minimal feature-resolution check so a declared feature cannot point
  at an absent module (`cargo check -p orbok-embed`).

**Acceptance:** `cargo fmt --check` passes at workspace level.

> **Out of scope (separate finding):** `cargo check -p orbok-embed
> --features tract` currently fails with a pre-existing `SimplePlan` import
> error in `tract_backend.rs`, unrelated to candle and untouched by RFC-046.
> Recorded in `rfcs/appendices/FINDING-tract-feature-build.md` for its own
> investigation. RFC-046 does not address it.

## 3. Tests

- Add a unit test in `orbok-embed`: building a config with
  `InferenceBackend::CandleCpu` (and `CandleCuda`) returns an error whose
  message matches the §10.1 contract and does **not** mention rebuilding
  with a feature.
- Keep the existing `tract` feature-flag-absent error test.
- `cargo test --workspace --lib` stays green.

## 4. Guardrails

- Do **not** remove the enum variants (that is B2, explicitly not selected).
- Do **not** add `#[deprecated]` yet — tracking note only.
- Do **not** alter the `tract` or `Mock` paths.
- Preserve CHANGELOG history; the v0.7.0 entry is correct and stays as-is.
  The new behavior is documented forward in `[0.22.0]`.

## 5. Release

- Version bump 0.21.0 → 0.22.0 (workspace + internal dep versions;
  third-party pins untouched).
- On ship: RFC-046 Status → `Implemented (v0.22.0)`, move
  `proposed/` → `done/`, update RFC index, ROADMAP, handoffs README.
- Package `orbok-v0.22.0.tar.gz` (flat; exclude target/.git/*.tar.gz).
