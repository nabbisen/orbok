# RFC-054: Runtime Data Override Profile Scope

**Project:** orbok\
**RFC:** 054\
**Title:** Runtime Data Override Profile Scope\
**Status:** Implemented (main at `6bcedd9`; release pending)\
**Target milestone:** v1.0.0 stabilization\
**Date:** 2026-08-04\
**Accepted:** 2026-08-04 by the project owner\
**Implemented:** 2026-08-04 — Slice 1 `c618a62`, Slice 2 `9a0ff3e`, cross-platform test placement and check-truthfulness fixes `6bcedd9` (Reviews 149, 150)\
**Related RFCs:** RFC-049 Portable Runtime Data Isolation (this narrows a gap in it); RFC-030 Portable Mode; RFC-039 Privacy Modes and Local Data Visibility; RFC-019 Test Matrix and Release Readiness\
**Handoff:** [`HANDOFF-054-runtime-data-override-profile-scope.md`](../handoffs/HANDOFF-054-runtime-data-override-profile-scope.md)

---

## 1. Summary

`ORBOK_DATA_DIR` relocates the catalog, cache, and model directories but not the
settings file. This RFC makes it relocate the whole standard profile, settings
included, so that one variable yields one complete profile on every supported
platform.

Default placement does not change on any platform. No new environment variable.
Behaviour changes only when the override is set.

## 2. Triggering evidence

### 2.1 The override splits the profile

`crates/app/src/runtime_context.rs`, in `RuntimeContext::resolve`:

```rust
let data_dir = match selection.mode {
    RuntimeMode::Portable => { /* alias checks */ portable_data_dir }
    RuntimeMode::Standard => {
        if let Some(override_path) = selection.standard_data_override.as_deref() {
            anchor_and_normalize(&startup_dir, Path::new(override_path))?
        } else { standard_default_data_dir }
    }
};
let settings_dir = match selection.mode {
    RuntimeMode::Portable => data_dir.clone(),
    RuntimeMode::Standard => standard_settings_dir,   // override never consulted
};
```

In Standard mode `selection.standard_data_override` is never consulted for
`settings_dir`. Portable mode already sets `settings_dir = data_dir`; the
standard-with-override path is the only one that produces a half-relocated
profile.

RFC-049's thesis is that a profile is an indivisible unit resolved once per
process. This is the single place where that does not hold.

### 2.2 The platform directories do not separate the way the design assumes

orbok derives its two roots independently: `dirs::data_local_dir()` for data
(`crates/app/src/bootstrap.rs`), and `app-json-settings`' `ConfigManager::new()`
for settings (`crates/app/src/settings.rs:100`). Read from the `dirs 6.0.0`
source in this workspace's registry, not from documentation:

| Platform | `data_local_dir()` | `config_dir()` | Same? |
|---|---|---|---|
| Linux (`lin.rs:9-12`) | `$XDG_DATA_HOME` or `~/.local/share` | `$XDG_CONFIG_HOME` or `~/.config` | no |
| macOS (`mac.rs:10-13`) | `app_support_dir()` | `app_support_dir()` | **yes — same function** |
| Windows (`win.rs:8-11`) | `%LOCALAPPDATA%` | `%APPDATA%` (Roaming) | no |

**On macOS the two are literally the same call.** orbok's settings and catalog
already occupy one directory there, so setting `ORBOK_DATA_DIR` *splits a
directory the platform deliberately unifies* — the opposite of the tidiness the
current behaviour is meant to preserve.

The data/config distinction that justifies today's behaviour is an XDG concept.
It is real on Linux, real-but-different on Windows (Roaming versus Local), and
absent on macOS. A cross-platform application should not encode it as universal.

Corroboration in existing code: the portable alias check at
`runtime_context.rs:105` tests `portable_data_dir` against both
`standard_default_data_dir` and `standard_settings_dir`. On macOS those are one
path, so the check compares against it twice. Harmless, but it is the conflation
showing through.

### 2.3 The existing workaround is Linux-only

Only `lin.rs` reads environment variables. `mac.rs` performs no environment
lookup at all, and `win.rs` uses known-folder APIs. Therefore setting
`XDG_CONFIG_HOME` alongside `ORBOK_DATA_DIR` — the isolation technique used
during RFC-049 development — **has no effect on macOS or Windows.**

Complete profile isolation via the environment is currently achievable on Linux
only. For an RFC whose entire subject is platform-parity isolation, that is a
gap in RFC-049 itself rather than a mere inconvenience.

### 2.4 It has already caused an incident

Review-request 113 disclosed that an RFC-049 development run wrote to this
machine's real `~/.config/orbok/settings.json` while `ORBOK_DATA_DIR` was set.
The dev team thereafter isolated `XDG_CONFIG_HOME` separately — a workaround
that, per §2.3, would not have protected a macOS or Windows contributor.

### 2.5 CI writes into the runner's real config directory

CI sets `ORBOK_DATA_DIR` for the headless `--check` run, and RFC-049's C4 fix
writes a default settings file when none exists. Each check run therefore
creates settings in the runner's real platform config directory rather than
under the override.

### 2.6 The isolation suite routes around the mechanism

`runtime_isolation_tests.rs` injects `PlatformRuntimePaths` directly rather than
exercising environment resolution. That is legitimate for unit-level
determinism, but it means the environment path — the one users and CI actually
take — has no cross-platform coverage. The suite passes on all three legs
without ever testing the thing that is broken.

### 2.7 The release-readiness "fresh" gate is not fresh

`docs/src/maintainers/release_readiness.md:35` instructs the maintainer to run
`ORBOK_DATA_DIR=<fresh-temp-dir> cargo run -p orbok -- --check`, and
`.github/workflows/ci.yml:117-119` does the same with an explicit
`rm -rf "$ORBOK_DATA_DIR"` beforehand. Both are described as establishing a
*fresh* profile.

Under current behaviour neither is. The `rm -rf` cannot reach the settings file,
because the settings file is not under `ORBOK_DATA_DIR`. On a CI runner this is
latent — the config directory starts empty — but for the maintainer running the
same gate locally before cutting a release it is active: **their real
`settings.json` participates in a check documented as clean-profile.** A gate
that silently tests something other than what it claims is the defect class this
project has been removing elsewhere; this is another instance of it.

## 3. Decision

When `ORBOK_DATA_DIR` is set and the process is in Standard mode, the settings
file resolves **under the override directory**, alongside the catalog, cache,
and model directories — the same relationship portable mode already has.

## 4. Required behaviour

1. **Standard with override** — `settings_dir` is the anchored, normalized
   override directory; `settings_file` is that directory joined with
   `SETTINGS_FILE`. Identical on all three platforms.
2. **Standard without override** — unchanged. The platform config directory via
   `settings::standard_settings_file()`.
3. **Portable** — unchanged.
4. `--portable` together with a non-empty `ORBOK_DATA_DIR` remains rejected.
5. When the override is set, the platform config directory must be **neither
   read nor written**. Not merely unused for the resolved path — untouched.

## 5. Non-goals

Each of these is a rule, not an omission.

1. **Default placement does not change on any platform.** Windows' Roaming/Local
   split is deliberate: settings roaming between domain-joined machines while a
   multi-gigabyte index does not is correct behaviour. macOS Application Support
   and Linux XDG likewise stay as they are. This RFC changes behaviour *only*
   when the override is set. Reading §3 as licence to unify the defaults is a
   misreading.
2. **No `ORBOK_CONFIG_DIR`.** Rejected explicitly — §6.
3. **No rename of `ORBOK_DATA_DIR`.** §6.
4. **No migration.** Nothing moves existing files. A user who has been running
   with an override keeps their existing settings where they are and gets a
   fresh default under the override; the loud alternative is worse than the
   quiet one here because the override is a development and test facility, not
   a supported end-user configuration.

## 6. Alternatives rejected

**A — Leave the behaviour, document it.** The behaviour is defensible on Linux
and indefensible on macOS, where it splits a single platform directory.
Documentation also cannot give macOS and Windows an isolation mechanism they do
not have (§2.3), so this option leaves §2.5 and the §2.4 incident class in
place.

**C — Add a separate `ORBOK_CONFIG_DIR`.** Rejected on cross-platform grounds
specifically: on macOS the two variables would default to the same directory —
two names for one path, settable inconsistently. It also *preserves* the ability
to half-isolate a profile, which is the defect this RFC exists to remove.

**D — Rename to `ORBOK_PROFILE_DIR`.** Same semantics as this RFC plus a
compatibility break. The name is a wart, not a defect; the variable is
documented as relocating orbok's data and under this RFC settings become part of
that data. Revisit only if a future major version breaks environment-variable
names for other reasons.

## 7. Cost

One capability is lost: running against a test catalog while keeping real
settings. A developer reproducing a configuration-dependent bug with a clean
index can currently do that with `ORBOK_DATA_DIR` alone; afterwards they must
copy `settings.json` into the override directory first.

That is a genuine regression for a genuine workflow, and it is stated here
rather than discovered later. It is accepted because the workflow has a one-line
replacement while the defect it depends on — a run that silently writes to the
user's live settings — does not.

## 8. Security and privacy

RFC-039's `privacy_mode` lives in `settings.json`. Under current behaviour, a
test or CI run given an override can read and rewrite the user's real privacy
setting. Afterwards, an overridden run cannot reach the user's settings file at
all.

This is the reason the change is worth making beyond tidiness: the split profile
is not just surprising, it is a path by which automated runs reach a
privacy-relevant user setting.

## 9. Testing requirements

1. **Standard with override** — settings resolve under the override, and the
   platform config directory is neither read nor written. Assert via RFC-049's
   existing `RuntimePathProbe` access seam, not by comparing paths: a path
   assertion cannot detect a stray read, and requirement §4.5 is about access,
   not about the resolved value.
2. **Standard without override** — resolution unchanged, per platform.
3. **Portable** — unchanged; existing alias rejections still fire.
4. **Demonstrated on all three CI legs, not inferred from Linux.** This is the
   precise failure mode the RFC exists to correct (§2.3, §2.6); a Linux-only
   demonstration would reproduce it.
5. The RFC-049 isolation suite's separate `XDG_CONFIG_HOME` handling should
   become unnecessary. If it does not, that is evidence the change is
   incomplete — report it rather than work around it.
6. **CI** — after the change, the headless `--check` run must leave the runner's
   platform config directory untouched. Assert this; do not assume it. §2.5 was
   invisible for two releases precisely because nothing checked.

## 10. Acceptance criteria

- [ ] `RuntimeContext::resolve` sets `settings_dir` from the override in
      Standard mode, with the portable and no-override paths unchanged.
- [ ] Tests for §9.1–§9.3, using the access seam for §9.1.
- [ ] The three-platform CI legs green, with §9.4 visible in the run rather than
      argued in the review request.
- [ ] §9.6 asserted in CI.
- [ ] `ORBOK_DATA_DIR`'s documentation — user docs and any `--help` text —
      states that it relocates the entire profile including settings, and that
      default placement is unaffected.
- [ ] RFC-049 amended with a dated note pointing here, recording that its
      isolation guarantee had this gap and that the environment path was
      untested cross-platform. RFC-049 is in `done/`; per RFC-000 the amendment
      is an appended note, not a body rewrite.

A developer handoff follows acceptance, per the 5-folder lifecycle.

## 11. Note to the reviewer of this RFC

§2.2's table is quoted from the `dirs 6.0.0` source in this workspace's registry
(`mac.rs:10-13`, `win.rs:8-11`, `lin.rs:9-12`), not from documentation. If
`dirs` changes major version, re-verify the table before relying on it — the
macOS equality is the load-bearing fact in this RFC and it is an implementation
detail of that crate, not a platform guarantee.
