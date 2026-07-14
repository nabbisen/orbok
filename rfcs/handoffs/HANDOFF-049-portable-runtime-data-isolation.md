# Implementation Handoff — RFC-049: Portable Runtime Data Isolation

**Project:** orbok  
**RFC:** 049  
**Lifecycle stage:** Design + handoff  
**Primary owner:** `orbok-app` bootstrap/runtime wiring  
**RFC:** [`../proposed/049-portable-runtime-data-isolation.md`](../proposed/049-portable-runtime-data-isolation.md)

> **Scope rule:** Establish one runtime context before touching persistent
> state. Do not combine this work with profile migration or unrelated bootstrap
> refactoring.

## 1. Expected Change Surface

- `crates/app/src/main.rs`
- `crates/app/src/bootstrap.rs`
- `crates/app/src/settings.rs`
- app tests in a sibling test module/directory
- user/maintainer documentation for portable precedence

## 2. Program Design

1. Add a small argument/runtime-mode type and a resolved runtime-context type.
2. Parse arguments before `load_initial_state` or `run_check`.
3. Treat an empty `ORBOK_DATA_DIR` as unset and reject a non-empty override with
   `--portable` before profile filesystem access.
4. Resolve the startup current directory to an absolute frozen anchor;
   `--portable` joins `orbok-data` to that anchor.
5. Resolve the data directory once; derive catalog, cache, models, settings,
   diagnostics, and temporary paths from it.
6. Introduce a narrow injectable access seam used by later catalog/settings/
   cache/model/recovery opens so tests can count or deny inactive-profile probes.
7. Change bootstrap/check/settings entry points to accept explicit context or
   explicit paths. Remove internal default-path re-resolution on those paths.
8. Capture the context in the iced application/update closures.
9. Make invalid portable setup fail closed.

The context should be immutable after construction and avoid becoming a general
service locator. It contains runtime locations/mode, not open repositories or
mutable application state.

## 3. Test Sequence

1. Unit-test argument/override precedence.
2. Build separate temporary standard and portable profiles with different
   sentinel settings, sources, history, and queued jobs.
3. Exercise initial-state load and recovery in each mode.
4. Exercise `--check` in each mode.
5. Use the access seam to assert zero open/probe calls for the inactive profile;
   separately assert its sentinels are unchanged and no files are created.
6. Change current directory after context construction and prove all paths stay
   anchored to the startup directory.
7. Exercise an unwritable/invalid portable directory and prove no fallback.

Avoid process-global environment races: isolate environment mutation in a
single-threaded subprocess test or inject the override into the resolver.

## 4. Review Slices

1. Runtime types, precedence tests, frozen path resolution, and injectable
   access-seam definition (no persistent opens).
2. Bootstrap/main/check propagation plus isolation tests.
3. Documentation and full-gate evidence.

## 5. Validation

- `cargo fmt --all --check`
- `cargo test -p orbok --lib` if an app library target is introduced, otherwise
  the narrow app test target selected by implementation
- `cargo test --workspace --lib`
- `cargo clippy --workspace --all-targets -- -D warnings`
- standard and portable fresh-directory headless checks
- `git diff --check`

## 6. Stop Conditions

Return to design review if isolation requires changing source-path semantics,
migrating existing data, introducing multiple live profiles, or redefining the
meaning of `ORBOK_DATA_DIR` beyond a deterministic precedence choice.

## 7. Definition of Done

One context controls the complete process, standard and portable sentinels
cannot cross-contaminate, `--check` obeys the selected mode, failure is closed,
documentation matches behavior, and an implementation review package records
the observed tests.
