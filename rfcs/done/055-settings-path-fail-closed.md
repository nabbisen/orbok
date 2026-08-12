# RFC-055: Fail-Closed Settings Path Resolution

**Project:** orbok\
**RFC:** 055\
**Title:** Fail-Closed Settings Path Resolution\
**Status:** Implemented (main at `9da7a4c`; release pending)\
**Target milestone:** v1.0.0 stabilization\
**Date:** 2026-08-04\
**Accepted:** 2026-08-04 by the project owner\
**Implemented:** 2026-08-08 — `8be890d`, with Review 151 §4's compatibility assertion fixed in `9da7a4c` (Reviews 151, 152)\
**Related RFCs:** RFC-049 Portable Runtime Data Isolation; RFC-054 Runtime Data Override Profile Scope; RFC-030 Portable Mode; RFC-053 rusqlite Line and Rust MSRV Policy (dependency-track precedent)\
**Supersedes decision:** none — establishes the `app-json-settings` usage contract\
**Handoff:** [`HANDOFF-055-settings-path-fail-closed.md`](../handoffs/HANDOFF-055-settings-path-fail-closed.md)

---

## 1. Summary

Move `app-json-settings` from the lockfile-pinned `2.0.3` to `2.6.0`, and change
orbok's single call site from `ConfigManager::new()` to
`ConfigManager::for_app("orbok")`.

This replaces a silent path substitution with a reported error at the one place
where orbok's settings directory is derived. It also decides what orbok does
when no platform configuration directory exists at all — a case portable mode
is specifically built for and which currently resolves to a working-directory
relative path.

No schema, storage format, or default-path change for any user whose executable
is named `orbok`.

## 2. Triggering evidence

### 2.1 The version we are pinned to already substitutes silently

We reported upstream that `2.0.3` panics on an unresolvable application name,
and that the silent fallback introduced in `2.1.0` was therefore a regression
for us. **That was only half right, and the half we got wrong is the half that
affects us.**

`2.0.3`'s `core/dir.rs`:

```rust
pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
```

With `HOME` unset, `default_config_dir()` yields `./.config` and `new()`
returns a **relative** path — `./.config/orbok/settings.json`. The panic we
described applies to `current_exe()` failing, not to a missing home directory.
So the silent-substitution class exists in the version we are pinned to, and has
since before we pinned it.

### 2.2 RFC-049 contains that, but does not remove it

`runtime_context.rs:108` passes the derived settings directory through
`anchor_and_normalize(&startup_dir, …)`, so a relative result is anchored to the
frozen startup directory rather than drifting with the working directory. The
outcome is deterministic and passes RFC-049's alias checks.

It is also wrong: a standard-mode run on a machine with no `HOME` writes the
user's settings beside wherever the binary happened to be launched from, and
reports success. RFC-049 converted an unbounded failure into a bounded one. It
did not make it correct.

### 2.3 The pin is one routine command deep

`Cargo.toml:65` declares `app-json-settings = { version = "2" }`. Only
`Cargo.lock` holds `2.0.3`. A `cargo update` adopts `2.6.0` with no code change
and no review — and `new()`'s behaviour there is the fallback we told upstream
was worse for us than a panic.

Our position of "staying on 2.0.3 deliberately" is not expressed anywhere a
reader or a tool can see. That is a policy held by accident.

### 2.4 Upstream found a data-integrity failure we did not

Investigating our report, the `app-json-settings` maintainers found that
`new()`'s fallback name is the fixed literal `"app"`, so **any two applications
that hit it share a settings file** and can overwrite each other. Their 2.6.0
documentation now states it directly:

> **This fallback is a fixed constant.** Any two executables that both hit it
> resolve to the same name, and therefore the same settings file […] one can
> silently read and overwrite the other's.

This is a stronger reason to leave `new()` than the one we raised.

### 2.5 What 2.6.0 provides

Verified by reading the fetched crate source, not the release notes:

- `for_app(app_name: &str) -> Result<Self>` (`core.rs:118`) — explicit name, no
  derivation, validated as a single safe path component.
- `try_new() -> Result<Self>` (`core.rs:102`) — the fallible constructor we
  asked for; not what orbok needs, since we do not want derivation at all.
- `folder_path(&self) -> &Path` (`core.rs:225`) — assert the directory before
  freezing it.
- `with_root_dir()` (`core.rs:154`) — documented remedy for "services or
  containers without a user environment."
- `ConfigError` has five variants and is **not** `#[non_exhaustive]`. orbok
  matches on it nowhere, so this is not a break for us.
- `rust-version = "1.85.0"`, below orbok's measured floor of 1.91. Not a
  constraint.

**The relative-path fallback is gone.** `default_config_dir()` now returns
`Result`, and its failure conditions differ per platform:

| Platform | Resolves from | Fails when |
|---|---|---|
| Linux | `XDG_CONFIG_HOME`, else `HOME`/`.config` | neither is set |
| macOS | `HOME`/`Library/Application Support` | `HOME` unset |
| Windows | `%APPDATA%` (environment variable) | `%APPDATA%` unset |

Note the Windows row: `app-json-settings` reads the **environment variable**,
where `dirs` — which orbok uses for the data directory — calls the known-folder
API. The two can therefore disagree on Windows, with `dirs` succeeding where
`app-json-settings` fails. This RFC does not attempt to reconcile them; it
records the asymmetry so a future reader does not assume symmetry.

RFC-054 §2.2's macOS equality is unaffected: `2.6.0` still resolves macOS
configuration to `Library/Application Support`, matching `dirs`.

## 3. Decision

1. Require `app-json-settings 2.6.0` and use
   `ConfigManager::<OrbokSettings>::for_app("orbok")`.
2. Resolution failure is **reported, never substituted**.
3. Failure is fatal **only when the resolved run actually uses the platform
   configuration directory** — that is, standard mode without an
   `ORBOK_DATA_DIR` override. Portable mode and standard-with-override start
   normally on a machine where no platform configuration directory exists.

Point 3 is the substance of this RFC. Points 1 and 2 follow from RFC-049's
existing principles; point 3 is a decision RFC-049 never had to make because
the old API could not fail.

## 4. Required behaviour

1. `settings::standard_settings_file()` becomes fallible. Its sole production
   caller is `bootstrap.rs:63`, inside `resolve_runtime_context()`, which
   already returns `Result`.
2. `PlatformRuntimePaths::standard_settings_dir` becomes optional. The capture
   phase remains **unconditional** — it resolves and records absence rather than
   branching on mode before resolving. RFC-049's thesis is that process inputs
   are captured once; a capture phase that already knows the mode is a different
   design and not this one.
3. `RuntimeContext::resolve` fails with a distinct error when the mode requires
   the platform settings directory and it is absent. The message must name the
   remedy (`--portable`, or `ORBOK_DATA_DIR`), because the environments that
   produce it — containers, service accounts, kiosks — are ones where the user
   is reading a log rather than watching a screen.
4. **When the platform settings directory is absent, RFC-049's portable alias
   check against it is skipped, deliberately.** There is no standard profile
   location to alias. Every other alias check is unaffected. This is a
   narrowing of an RFC-049 safety check and must be explicit in code and
   covered by a test, not an emergent consequence of an `Option`.
5. No default path changes for an executable named `orbok`:
   `for_app("orbok")` and `new()`'s derived stem both yield
   `<config>/orbok/settings.json`.

## 5. Non-goals

1. **No adoption of `load`/`save`.** orbok's settings I/O goes through
   `runtime_storage`'s boundary per RFC-049. We use this crate for path
   derivation only, and that stays true. `SaveMode::Atomic` being 2.6.0's
   default is irrelevant to us for the same reason.
2. **No `with_root_dir()` fallback.** Supplying a root when the platform cannot
   provide one would reintroduce substitution under a different name. Portable
   mode and `ORBOK_DATA_DIR` are orbok's supported answers to "no user
   environment," and both already exist.
3. **No migration.** Nothing moves existing settings files.
4. **No `try_new()`.** It solves derivation-with-failure; orbok wants no
   derivation.

## 6. Alternatives rejected

**A — Stay on 2.0.3.** §2.1 and §2.3 remove the basis for this. The behaviour
we were holding out against is present in the pinned version, and the pin is not
expressed where anyone would see it.

**B — Upgrade but keep `new()`.** Adopts the `"app"` collision (§2.4) for no
gain. `new()` is the wrong constructor for an application whose directory
identity is load-bearing, which is the finding we sent upstream.

**C — Fail startup whenever resolution fails, regardless of mode.** Simplest,
and wrong. Portable mode exists to run without a user profile; a container or
kiosk with no `HOME` is its intended habitat, not an edge case. Failing there
would break the mode in the environment it was designed for, in service of a
path that run never consults.

**D — Resolve conditionally, only in the mode that needs it.** Same end state as
the decision, reached by branching before capture. Rejected on RFC-049 grounds:
capture once, uniformly, then decide. Resolve-then-tolerate keeps the capture
phase mode-agnostic and puts the mode-specific rule in exactly one place.

## 7. Cost

A user who **renamed the executable** sees their settings directory change.
`new()` derived it from the file stem, so `orbok-beta` wrote to
`<config>/orbok-beta/`; `for_app("orbok")` writes to `<config>/orbok/`. Their
old settings are not migrated and appear reset.

Stated rather than discovered later. Accepted because a settings location that
silently changes when a file is renamed is the defect, not the feature — it is
the same substitution class this RFC removes — and because the affected
population is users who renamed a binary and never noticed the consequence.

## 8. Security and privacy

RFC-039's `privacy_mode` lives in `settings.json`. Two improvements:

- A standard-mode run on a machine without a home directory currently writes
  that file beside the launch location, where its confidentiality depends on
  whatever directory the binary sat in. After this change it does not run at all.
- The `"app"` collision (§2.4) is a path by which an unrelated application could
  read orbok's privacy settings, or orbok could overwrite another
  application's. orbok reaches that fallback only if `current_exe()` fails, so
  the exposure is narrow — but it is removed rather than bounded.

## 9. Testing requirements

1. **`for_app("orbok")` resolves to the same path `new()` did** on each
   platform, proving §4.5. This is the compatibility claim; it must be measured,
   not reasoned.
2. **Absent platform config directory, standard mode without override → startup
   fails** with the distinct error, and the message names a remedy.
3. **Absent platform config directory, portable mode → starts normally.**
4. **Absent platform config directory, standard mode with `ORBOK_DATA_DIR` →
   starts normally.**
5. **§4.4's skipped alias check is tested directly:** portable mode with an
   absent standard settings directory must still reject every other alias it
   rejects today. A test that only proves startup succeeds would pass even if
   the whole alias check were removed.
6. Tests 2–4 must not mutate process environment variables — that is `unsafe` in
   this edition and races the parallel harness. Drive them through the injected
   `PlatformRuntimePaths` seam, as RFC-049's suite already does. Upstream faced
   the same constraint and solved it with `config_dir_from`'s injection seam;
   the same shape applies here.
7. **All three CI legs**, per RFC-054 §9.4 — and note that the required test
   group must actually run there. RFC-054's implementation review found
   `runtime_context/tests.rs` executing on Linux only because the cross job runs
   the bin target alone. Confirm placement rather than assuming it.

## 10. Acceptance criteria

- [ ] `Cargo.toml` requires `app-json-settings 2.6.0`; `Cargo.lock` updated;
      the manifest carries a comment recording *why* the version floor exists,
      so §2.3 cannot recur silently.
- [ ] `settings.rs` uses `for_app("orbok")`; `new()` appears nowhere.
- [ ] `PlatformRuntimePaths::standard_settings_dir` is optional and §4.3's
      error exists with a remedy-naming message.
- [ ] §4.4's skipped alias check is explicit in code and covered by §9.5.
- [ ] Tests §9.1–§9.5, running on all three legs per §9.7.
- [ ] `docs/src/intermediate/settings.md` documents what happens when no
      platform configuration directory exists, and which modes still work.
- [ ] CHANGELOG entry covering the renamed-executable cost (§7).

A developer handoff follows acceptance.

## 11. Note to the reviewer

Everything in §2.5 was read from `app-json-settings-2.6.0`'s source fetched into
a scratch project, not from its release notes or the maintainers' reply — the
signatures, the per-platform failure table, the `ConfigError` variants, and the
absence of `#[non_exhaustive]`. The upstream reply and the crate agree, but the
table in §2.5 is the artifact to re-verify if the version moves, since the
Windows `%APPDATA%`-versus-known-folder asymmetry is the kind of detail release
notes do not carry.

We should also send a short correction upstream: our original report told them
`2.0.3` panics, and §2.1 shows it substitutes for the `HOME` case. Their RFC 037
amendment describes the fallback as a behavioural change introduced in 2.1.0,
which is not accurate for that case, and they acted on our report in good faith.

## 12. Amendment (2026-08-08, Task 008): the floor moves to 2.7.0 for the checked filename setter

§3.2's migration, as given in the maintainers' first reply, used
`try_with_filename("settings.json")?` — the validated setter. What shipped used
`with_filename("settings.json")`, the unvalidated one. Both compile identically
against `OrbokSettings`'s fixed literal, so the difference produced no defect:
`"settings.json"` passes every check either setter would apply. But it left a
gap between what this RFC decided and what the code does, and the gap survived
three implementation reviews (151, 152, 153) because each checked the code
against [HANDOFF-055](../handoffs/HANDOFF-055-settings-path-fail-closed.md)
§3.2, which named only the constructor and said nothing about the filename
setter — not against the upstream reply the handoff was itself derived from.
The handoff had become the review baseline, and the source it was derived from
stopped being consulted.

Task 008, a dev-team task directive (not linked here — task files live under
the untracked `.git-exclude/` directory and are not part of the release
archive `rfcs/` ships in) closes the gap: `standard_settings_file()` now uses
`try_with_filename`, and the dependency floor moves `2.6.0 → 2.7.0`. The setter change alone needed no version
move — `try_with_filename` exists in 2.6.0 — but 2.7.0 is where
`validate_plain_file_name` starts rejecting Windows reserved device names
(`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, case-insensitively and
as a stem with an extension, so `nul.json` too). Nothing depends on that check
today — the filename is a fixed literal, not derived from any input — but it is
the version in which the checked setter actually carries the check that matters
on one of orbok's three platforms, and the two changes share a documentation
cost cheaper paid once.

Nothing else in this RFC's decision, required behavior, or acceptance criteria
changes.

## 13. Amendment (2026-08-12, Task 019): `standard_settings_file()` becomes `standard_settings_dir()`

§10's acceptance criteria and this RFC's own prose name
`standard_settings_file()`. It now returns the settings **directory**:
`settings::standard_settings_dir()`, via `ConfigManager::folder_path().to_path_buf()`
rather than `try_with_filename("settings.json")?.path()`.

The function's sole production caller (`bootstrap::resolve_runtime_context`)
never wanted the file — it immediately discarded the filename with
`.parent()`, since `RuntimeContext` re-derives the file path itself as
`settings_dir.join(SETTINGS_FILE)`. Asking for the file at all was based on
an incorrect belief, reported by us to the `app-json-settings` maintainers
and corrected by them, that `folder_path()` would require keeping the
`ConfigManager` alive past this call — RFC-049 forbids that. It does not:
`folder_path()` borrows from the manager, `.to_path_buf()` copies out within
the same expression, and the manager drops at the end of the statement.

This is not a regression on §12's checked-setter fix: `try_with_filename` is
no longer called at all, which is stronger than passing a checked filename —
one that is never passed cannot be the wrong one.

Nothing else in this RFC's decision, required behavior, or acceptance
criteria changes.
