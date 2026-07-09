# Finding Note: `tract` feature does not compile (`SimplePlan` unresolved)

**Project:** orbok
**Type:** Finding note (pre-investigation). **Not an RFC. No decision recorded.**
**Discovered:** 2026-06-30, during RFC-046 (candle backend) implementation.
**Related:** RFC-046 (candle backend); RFC-021 (embedding model selection).

---

## 1. What was observed

While verifying RFC-046's acceptance criteria, `cargo check -p orbok-embed
--features tract` failed:

```
error[E0425]: cannot find type `SimplePlan` in this scope
  --> crates/search/embed/src/tract_backend.rs:30:12
   |
30 |     model: SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>,
```

`tract_backend.rs` imports `use tract_onnx::prelude::*;`, but `SimplePlan`
is not brought into scope by that glob in the pinned `tract-onnx` 0.23.

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

## 3. Current impact (stated neutrally)

- Default builds and `cargo test --workspace --lib` are unaffected — the
  `tract` feature is off by default, so `tract_backend` is not compiled.
- `--features tract` does not currently compile.
- The recommended default model (`multilingual-e5-small`, RFC-021) is an
  ONNX model intended to run via the `tract` `OnnxRuntime` path. So unlike
  candle (a never-built secondary slot), `tract` is the *intended real*
  inference path — which makes its build state more consequential than
  candle's, and worth its own careful investigation rather than a quick fix.

## 4. Not yet established (open for investigation)

Mirroring the discipline applied to candle, the following should be checked
before any change, **not** assumed:

1. Whether `SimplePlan` is the only unresolved symbol, or the first of
   several (i.e. is `tract_backend.rs` one import short, or materially drifted
   from `tract-onnx` 0.23's API?).
2. Whether `--features tract` has *ever* compiled in any released version
   (check release tarballs, as was done for candle), or whether it has been
   latent like candle.
3. What the CHANGELOG and RFC-021/RFC-008 actually scoped for the tract path
   (selection criterion vs. delivered, verified backend).
4. The correct fix surface: a one-line import, a broader API-drift update, or
   a version re-pin.

## 5. Suggested disposition

Open a dedicated investigation (and, if a code/contract change is warranted,
an RFC — next free number at that time) once RFC-046 has shipped. Apply the
same feature-contract principle RFC-046 §12 records:

> A declared Cargo feature must compile, or it must not exist.

No fix is applied here, and no conclusion about cause is asserted. This note
exists so the finding is not lost.
