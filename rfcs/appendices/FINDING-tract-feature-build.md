# Finding Note: `tract` feature build recovery

**Project:** orbok
**Type:** Finding note. **Not an RFC. No decision recorded.**
**Discovered:** 2026-06-30, during RFC-046 (candle backend) implementation.
**Resolved:** 2026-07-10, by aligning the runnable plan type with `tract-core`
0.23.3.
**Related:** RFC-046 (candle backend); RFC-021 (embedding model selection).

---

## 1. What was observed originally

While verifying RFC-046's acceptance criteria, `cargo check -p orbok-embed
--features tract` failed:

```
error[E0425]: cannot find type `SimplePlan` in this scope
  --> crates/search/embed/src/tract_backend.rs:30:12
   |
30 |     model: SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>,
```

The source used an old/incorrect `SimplePlan` shape:

```rust
SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>
```

In the pinned `tract-core` 0.23.3 API, `SimplePlan` has two generic
parameters, and the typed runnable alias is:

```rust
TypedSimplePlan = SimplePlan<TypedFact, Box<dyn TypedOp>>
```

`into_runnable()` returns this runnable behind `Arc`.

## 2. Scope boundary (why this is a separate note)

This is **not** part of RFC-046. RFC-046 is scoped solely to the *candle*
backend (declared-but-never-implemented module). The `tract` issue is a
different backend with a different cause:

- **Candle:** the module file never existed; `--features candle` could never
  resolve the `mod`. (Resolved by RFC-046 / B1.)
- **tract:** the module file *exists* and is wired, but its source does not
  compile under the feature due to an unresolved import (and possibly other
  drift against the pinned `tract-onnx` version — not yet fully checked).

`tract_backend.rs` was **not** modified by RFC-046. This breakage is
pre-existing and independent.

## 3. Current impact

The declared `tract` feature now compiles again:

```sh
cargo check -p orbok-embed --features tract
```

The fix is intentionally narrow: `tract_backend.rs` stores the loaded runnable
as `Arc<TypedSimplePlan>`, matching the current `tract-core` API.

This resolves the feature-build contract issue. It does **not** prove the
backend is production-ready semantic inference. `embed_batch` still uses the
documented placeholder vector path rather than running tokenizer output through
the loaded ONNX graph.

## 4. Investigation result

The original open questions are now answered as follows:

1. `SimplePlan` was the only observed compile blocker in the feature build.
2. The local Cargo registry contains earlier published `orbok-embed` sources
   with the same old field type, suggesting the issue was latent across
   releases that carried this source shape.
3. RFC-046 remains correctly scoped to Candle; this fix is independent of the
   Candle removal.
4. The correct fix surface was a small API-drift update, not a `tract` re-pin.

## 5. Verification observed

- `cargo fmt --check`
- `cargo check -p orbok-embed`
- `cargo test -p orbok-embed --lib`
- `cargo check -p orbok-embed --features tract`

The broader real-inference work remains separate: configure tokenizer/model
inputs, run the ONNX graph in `embed_batch`, and validate output quality and
latency against the RFC-021 expectations.
