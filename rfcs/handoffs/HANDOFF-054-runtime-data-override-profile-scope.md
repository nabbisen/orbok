# Implementation Handoff — RFC-054: Runtime Data Override Profile Scope

**Project:** orbok\
**RFC:** 054\
**Lifecycle stage:** Implemented with RFC-054 on `main` at `6bcedd9`; release pending\
**Primary owner:** `crates/app` runtime context; user and maintainer docs; CI\
**RFC:** [`../done/054-runtime-data-override-profile-scope.md`](../done/054-runtime-data-override-profile-scope.md)

> **Scope rule:** This changes what `ORBOK_DATA_DIR` covers. It does **not**
> change where anything lives by default on any platform, does not add an
> environment variable, and does not move existing files. If a change you are
> making would alter a default path, you have left the scope — stop and report.

## 1. Expected change surface

**Production code — one site.**

- `crates/app/src/runtime_context.rs` — the `settings_dir` match, Standard arm.

That is not an estimate. `settings::standard_settings_file()` has exactly one
production caller (`crates/app/src/bootstrap.rs:63`); nothing else in the
workspace re-derives a settings path. RFC-049's sealed boundary is why this is a
one-line change rather than a hunt, and it is worth noticing that the design
held.

**Also in scope:**

- `crates/app/src/runtime_context/tests.rs` and
  `crates/app/src/runtime_isolation_tests.rs` — new coverage per RFC §9
- `crates/app/src/runtime_context.rs:5` — the module doc comment still says the
  override applies *"when bootstrap propagation is implemented."* It is
  implemented. Correct it while you are in the file.
- `README.md:46`, `docs/src/users/quick_start.md:37`,
  `docs/src/intermediate/settings.md:33` — each describes the variable as
  overriding the *data* directory
- `docs/src/maintainers/release_readiness.md:35` — see §3 below
- `.github/workflows/ci.yml` — §9.6 assertion
- `CHANGELOG.md`
- `rfcs/done/049-portable-runtime-data-isolation.md` — appended note (RFC §10)

**Not in scope:** `crates/pipeline/workers/src/tests/v08_features.rs:312`
removes `ORBOK_DATA_DIR` from the environment in a test. Leave it; it is
unrelated hygiene.

## 2. Program design

### Slice 1 — resolution, tests, docs

1. In the Standard arm of the `settings_dir` match, use the anchored override
   directory when `selection.standard_data_override` is present; fall through to
   `standard_settings_dir` when it is not. `settings_file` continues to be
   `settings_dir.join(SETTINGS_FILE)`, so no separate change is needed there.
2. Tests per RFC §9.1–§9.3. **§9.1 must use the `RuntimePathProbe` access seam,
   not a path comparison.** The requirement is that the platform config
   directory is *not accessed*; asserting on the resolved path would pass even
   if something read the real settings file on the way. This is the same
   distinction the RFC-049 reviews kept landing on.
3. §9.5 — the existing isolation suite's separate `XDG_CONFIG_HOME` handling
   should now be unnecessary. Remove it. **If removing it breaks a test, do not
   work around it — report.** A test that still needs `XDG_CONFIG_HOME` after
   this change is telling you the override is not covering something, and that
   signal is worth more than a green suite.
4. Docs, including the RFC-049 appended note.

### Slice 2 — CI assertion

RFC §9.6: after the change, the headless `--check` run must leave the runner's
platform config directory untouched.

The Rust-level proof is §9.1; this is the end-to-end check on the real binary,
so keep it cheap. Approach is yours. One caution: **do not implement it by
setting `HOME`/`APPDATA` per platform.** That reintroduces exactly the
platform-specific environment juggling this RFC exists to eliminate, and it
would make the CI check pass by a mechanism users do not have.

If you cannot assert this without adding production surface (a new flag, a path
dump in `--check` output), stop and report rather than adding it. Slice 1 is
independently valuable and can ship without Slice 2.

## 3. Why `release_readiness.md` is in the surface

RFC §2.7. That document tells the maintainer to run
`ORBOK_DATA_DIR=<fresh-temp-dir> cargo run -p orbok -- --check` and calls it a
fresh-profile gate. Today it is not one for a local run: the maintainer's real
`settings.json` participates. After this change it becomes what it already
claims to be.

The doc text may need no edit at all — check whether it does before changing it.
It is listed here so you verify the claim rather than inherit it.

## 4. Cross-platform requirement

RFC §9.4: **demonstrate on all three CI legs.** Do not infer macOS and Windows
behaviour from a Linux run and do not argue it in the review request — a
Linux-only demonstration is a reproduction of the bug, not evidence against it.

The RFC's §2.2 table is the reason: the three platforms genuinely differ, and on
macOS the two directories are the same path. Expect that a macOS assertion
comparing "settings dir" against "default data dir" will behave differently from
the Linux one, because before the change those are equal there. If a test you
write is trivially true on macOS, say so rather than letting it stand as
coverage.

## 5. Verification

- `cargo fmt --all --check`, `cargo clippy … -D warnings`, workspace tests
- `bash scripts/check-rfc-lifecycle.sh` (RFC-049's note and RFC-054's own
  eventual move both touch it)
- The three-platform CI legs green, with §9.4 visible in the run

## 6. Stop conditions

Report rather than proceeding if any of these hold:

1. The `settings_dir` change turns out to need more than the one match arm.
2. Removing the isolation suite's `XDG_CONFIG_HOME` handling breaks a test.
3. §9.6 cannot be asserted without new production surface.
4. Any default path changes on any platform.

Each of these means the design assumption behind this RFC was wrong somewhere,
and that is worth a conversation rather than a workaround.
