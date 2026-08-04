# Implementation Handoff — RFC-055: Fail-Closed Settings Path Resolution

**Project:** orbok\
**RFC:** 055\
**Lifecycle stage:** Accepted 2026-08-04; implementation not started\
**Primary owner:** workspace manifests; `crates/app` settings and runtime context\
**RFC:** [`../accepted/055-settings-path-fail-closed.md`](../accepted/055-settings-path-fail-closed.md)

> **Scope rule:** This changes how the settings *path* is derived and what
> happens when it cannot be. It does **not** adopt `load`/`save` from the
> crate — settings I/O stays on `runtime_storage`'s RFC-049 boundary — and it
> does not change any path for a user whose executable is named `orbok`. A
> change that moves an existing user's settings is out of scope; stop and
> report.

## 1. Expected change surface

- `Cargo.toml` — `app-json-settings` requirement and a comment recording the
  floor's reason (RFC §10 criterion 1)
- `Cargo.lock`
- `crates/app/src/settings.rs:100` — `standard_settings_file()`
- `crates/app/src/runtime_context.rs` — `PlatformRuntimePaths`, `resolve`, the
  portable alias check, and a new error variant
- `crates/app/src/bootstrap.rs:63` — the sole caller
- `crates/app/src/runtime_context/tests.rs` — §9.1–§9.5 coverage
- `docs/src/intermediate/settings.md`, `CHANGELOG.md`

**Also fold in, since you are in the file:** `standard_settings_file()`'s doc
comment currently reads *"Load settings from the platform config directory, or
return defaults if the file does not exist yet."* The function loads nothing and
returns a `PathBuf` — the comment describes behaviour that moved to
`runtime_storage` during RFC-049. Correct it rather than carrying it forward.

## 2. Why this is one slice, not two

`for_app()` **does not exist in 2.0.3** — verified against the pinned source in
the registry. So the version bump and the call-site change cannot be separated:
there is no intermediate state where the new constructor is available under the
old pin.

Splitting along the other seam is worse. If the dependency and `for_app()`
landed first, with fallible resolution but without §4's mode-dependent
tolerance, the intermediate commit would **break portable mode on any machine
without a home directory** — the environment portable mode exists for. An
intermediate state that breaks a supported mode is not a smaller review, it is a
regression with a scheduled fix.

Do it as one slice. Docs and CHANGELOG may follow as a second, trivial one if
you prefer.

## 3. Program design

1. **Bump the dependency** to `2.6.0` and record the floor's reason in the
   manifest comment. RFC §2.3 is the point: `version = "2"` let a routine
   `cargo update` adopt behaviour we had deliberately rejected, and nothing in
   the tree said so. The comment is the fix for that, not decoration.
2. **`standard_settings_file()` → `for_app("orbok")`,** returning `Result`. The
   literal `"orbok"` is the application identity; it is not derived from
   anything and must not become derived.
3. **`PlatformRuntimePaths::standard_settings_dir` becomes
   `Option<&'a Path>`.** The struct is `Clone, Copy` over borrowed paths, so
   this stays `Copy` — no ownership churn.
4. **Capture stays unconditional** (RFC §4.2). Resolve in `bootstrap.rs`,
   record `None` on `ConfigError::Platform`, and pass it through. Do not branch
   on mode before resolving — RFC-049's design is capture-once-then-decide, and
   a capture phase that already knows the mode is a different architecture.
5. **`RuntimeContext::resolve` fails** with a new error variant when the
   resolved mode needs the platform settings directory and it is `None` —
   that is, Standard mode with no `ORBOK_DATA_DIR`. Portable and
   Standard-with-override proceed.
6. **The portable alias check narrows** (RFC §4.4). When
   `standard_settings_dir` is `None`, skip only that comparison. Every other
   alias check still fires.

## 4. The two places to be careful

**4.1 — The error message is the whole user experience here.**

The environments that produce this failure are containers, service accounts and
kiosks. Whoever hits it is reading a log, not looking at a window, and they will
not know that orbok has a portable mode. The message must name the remedy:
`--portable`, or `ORBOK_DATA_DIR`. A message that says only "could not resolve
the platform configuration directory" hands the user the upstream crate's
problem statement and none of orbok's answer.

**4.2 — §4.4 narrows an RFC-049 safety check, so make it visible.**

Write the skip as an explicit, commented branch, not as a side effect of `if let
Some(...)`. Someone reading this in a year must be able to see that the check
was *decided* to be inapplicable rather than accidentally not run.

RFC §9.5 exists for the same reason: a test that only proves portable mode
starts would pass if the entire alias check were deleted. Prove the other
rejections still fire with the settings directory absent.

## 5. Testing

RFC §9.1–§9.5, with two constraints that matter more than usual:

**Do not mutate process environment variables.** It is `unsafe` in this edition
and races the parallel harness. Drive the absent-directory cases through the
injected `PlatformRuntimePaths` seam, as RFC-049's suite already does. Upstream
hit the identical constraint and solved it with `config_dir_from`'s injection
seam — worth reading `app-json-settings-2.6.0/src/core/dir.rs` for the shape.

**§9.1 is the compatibility claim and must be measured.** That `for_app("orbok")`
and the old `new()` resolve to the same path is what makes this a non-migration
for existing users. Assert it; do not reason it from the executable's name.

**Placement, per RFC §9.7.** Review 149 found `runtime_context/tests.rs` running
on Linux only, because the `cross` job ran the bin target alone. That is fixed —
`cargo test -p orbok --lib --locked` now runs there — so new tests in that file
will get three-platform coverage. Confirm it in the run rather than assuming it,
because that assumption is exactly what failed last time.

## 6. Stop conditions

1. `for_app("orbok")` resolves to a different path than `new()` did on any
   platform. That would make this a migration, which is out of scope.
2. Making `standard_settings_dir` optional turns out to touch alias checks
   beyond the one named in RFC §4.4.
3. Any `ConfigError` variant needs matching — the RFC assumes we match on none,
   and if that changed, `#[non_exhaustive]`'s absence becomes our problem.
4. The workspace needs `unsafe` env mutation to test any case in §5.

Each means an assumption behind the RFC was wrong, and is worth a conversation
rather than a workaround.

## 7. Related, not blocking

RFC §11 notes we owe the `app-json-settings` maintainers a correction: we told
them 2.0.3 panics, and it substitutes for the `HOME` case. That is the
architect's to send; it does not gate this work.
