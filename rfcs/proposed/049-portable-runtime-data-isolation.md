# RFC-049: Portable Runtime Data Isolation

**Project:** orbok  
**RFC:** 049  
**Title:** Portable Runtime Data Isolation  
**Status:** Proposed  
**Target milestone:** v1.0.0 stabilization  
**Date:** 2026-07-14  
**Related RFCs:** RFC-001 Local Data Classification and Lifecycle; RFC-030 Portable Mode; RFC-040 Safe Diagnostics  
**Handoff:** [`HANDOFF-049-portable-runtime-data-isolation.md`](../handoffs/HANDOFF-049-portable-runtime-data-isolation.md)

---

## 1. Summary

This RFC requires one immutable runtime data context to be resolved before any
catalog, cache, settings, model, diagnostics, or recovery operation occurs.
Standard and portable modes must use that same context for the entire process.

This closes a v1.0.0-blocking isolation defect: `--portable` currently affects
later operations, while initial state loading, startup recovery, and headless
checking can still use the standard profile.

## 2. Triggering Evidence

The architecture preparation review
`.git-exclude/reviewed/055-architect-preparation-review.md` found that
`main` calls `load_initial_state()` before applying the portable data directory.
It also found that `--check --portable` reaches `run_check()` without the
portable selection. A single process can therefore read or mutate two profiles.

## 3. Decision

orbok must parse runtime mode first and construct one `RuntimeContext` (the
exact Rust name may differ) containing at least:

- selected mode: standard or portable;
- resolved data directory;
- catalog path;
- cache path;
- models directory;
- settings location;
- temporary/support-output locations where applicable.

Every startup and runtime service receives paths derived from this context.
No startup function may independently call a default-directory resolver after
the context exists.

## 4. Required Behavior

1. Argument parsing and runtime-context resolution happen before database,
   settings, model, logging-to-file, recovery, or GUI initialization.
2. `--portable` uses only the portable context for initial state, recovery,
   history, sources, settings, models, cache, cleanup, and later mutations.
3. `--check --portable` validates only the portable context.
4. Standard mode behavior and locations remain unchanged.
5. Selecting portable mode never probes, opens, migrates, or modifies the
   standard catalog or settings as a fallback.
6. A missing portable directory may be created, but failure must be reported
   without falling back to standard mode.
7. User-visible diagnostics identify the selected mode without exposing more
   path detail than the active diagnostics/privacy policy permits.

## 5. Boundary and Compatibility Rules

- `orbok-app` owns argument parsing and platform path resolution.
- UI state receives data, not filesystem resolvers.
- Backend services receive explicit paths or a narrow context reference.
- `ORBOK_DATA_DIR` remains the explicit test/development override for standard
  mode. Supplying both `--portable` and a non-empty `ORBOK_DATA_DIR` is rejected
  as an ambiguous configuration; orbok must not silently choose either path.
- An absent or empty `ORBOK_DATA_DIR` is unset.
- `./orbok-data` means the startup current working directory joined with
  `orbok-data`. The startup directory is resolved to one absolute normalized
  anchor and frozen in `RuntimeContext`; later current-directory changes cannot
  redirect the profile. Failure to resolve the startup anchor fails closed.
- No migration between standard and portable profiles is introduced here.
- Source paths remain governed by RFC-003 and RFC-030.

## 6. Security and Privacy

The selected profile is a privacy boundary. Cross-profile reads can reveal
source names, recent searches, model configuration, and storage state; writes
can migrate or recover the wrong database. Tests must therefore detect reads
and writes, not only compare the final directory string.

Runtime path opening must pass through an injectable access seam (narrow opener
traits/functions or an equivalent subprocess-deny boundary). Tests assert zero
open/probe calls for every inactive-profile catalog, settings, cache, model,
recovery, and diagnostics path. Unchanged sentinels remain a second line of
evidence; they do not prove absence of reads.

## 7. Non-Goals

- Copying or synchronizing data between modes.
- Relative source-path portability.
- Multiple simultaneously active profiles.
- Changing portable-mode warnings or storage format beyond what isolation
  requires.
- Refactoring unrelated bootstrap responsibilities.

## 8. Testing Requirements

1. Standard startup creates/opens only the standard profile.
2. Portable startup creates/opens only the portable profile.
3. Distinct sentinel sources, settings, and history remain isolated.
4. Startup recovery in portable mode cannot mutate a queued/running job in the
   standard catalog.
5. `--check --portable` reports the portable schema and leaves the standard
   profile byte-for-byte or logically unchanged.
6. Invalid portable-path setup fails closed without standard fallback.
7. Argument tests prove standard mode honors `ORBOK_DATA_DIR`, portable mode
   uses `./orbok-data`, and the combined configuration is rejected before any
   profile is opened.
8. Access-seam tests assert zero inactive-profile open/probe calls across
   initial state, recovery, `--check`, and representative later operations.
9. A current-directory change after context construction does not change any
   resolved path.

## 9. Acceptance Criteria

This RFC is accepted when the runtime-context decision and isolation test
matrix are approved.

It is implemented when all startup and runtime paths use one resolved context,
the isolation tests pass, headless checks cover both modes, documentation
matches the final precedence rule, and an architecture review finds no
cross-profile access path.
