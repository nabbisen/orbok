# Implementation Handoff — RFC-041 §10.4 (part): opening a result, and showing it in its folder

**Project:** orbok\
**RFC:** 041 — Search, Narrow and Browse Around (§10.4); also RFC-038 §9.3's `[Open file]` and its `OpenAnyway` / `ShowInFolder` recovery actions\
**Lifecycle stage:** Accepted. This handoff covers opening files only; filters, the preview pane and the other browse-around actions remain RFC-041's later work.\
**Primary owner:** `crates/app/src/main.rs` (handlers), a new small module for launching, `crates/ui/src/views.rs` (result row)\
**RFC:** [`../accepted/041-search-narrow-and-browse-around.md`](../accepted/041-search-narrow-and-browse-around.md)

Owner priority 2, 2026-09-16 — ahead of the trust badges, filters and startup message.

---

## 0. The gap

**orbok cannot open a document it found.** Pressing a result sends
`Message::SelectResult(i)`, which sets `selected_result`
(`crates/ui/src/state.rs`); the row is styled as selected
(`views.rs:394`) and nothing else happens. No message, handler or dependency
anywhere opens a file or reveals it in a file manager. Enter is not bound for
results.

Both accepted RFCs already specify it: RFC-041 §10.4's browse-around flow ends
in `├─ Open file`, and RFC-038 §9.3 shows `[Open file]` on a result, with
`OpenAnyway` and `ShowInFolder` as recovery actions.

**This is the first time orbok launches anything outside itself**, so the
boundary matters more than the button.

## 1. The boundary — non-negotiable

1. **Validate immediately before launching.** The path is the result's stored
   canonical path, passed through the same `PathGuard` the snippet path uses
   since RFC-060 Slice 2: it must still be inside a registered, searchable
   source. A result whose path fails validation is **not** opened, and the
   user sees the existing file-not-found wording, not an error string.
2. **Never through a shell.** No `sh -c`, no `cmd /c`, no string
   interpolation. The launcher receives the path as one argument.
3. **Only paths that came from a result.** Nothing typed, nothing from a
   clipboard, nothing from a message the user could craft.
4. **Record the TOCTOU limitation, do not solve it.** Validation then launch
   has the same check-then-use window RFC-060 §9 already accepts for snippets.
   Say so in a comment.

## 2. The mechanism

- **Dependency:** the `opener` crate is the obvious candidate for
  open-in-default-application and reveal-in-file-manager on all three
  platforms. **Before adding it, verify and report:** that it launches without
  a shell on each platform; what its reveal support pulls into the dependency
  graph on Linux (a D-Bus stack would be a significant addition); and that
  `cargo audit` and `scripts/check-audit-ignores.sh` stay green. If reveal is
  heavy, ship open-file first and report reveal separately.
- **A launcher seam:** one trait, `Launcher { fn open(&ValidatedPath);
  fn reveal(&ValidatedPath); }`, with the real implementation in production
  and a recording fake in tests. No test may launch a real application.

## 3. Where it appears

There is no preview pane yet (that is RFC-041's later work). Until there is:

- The **selected** result row shows two actions: **Open file** and **Show in
  folder**.
- **Keyboard:** with a result selected and focus not in a text input, Enter
  opens it. Check `crates/ui/src/tests/a11y.rs`'s
  `key_map_enter_confirms_by_context` and the key map in
  `crates/ui/src/shell.rs` first — Enter already means other things in other
  contexts, and those must not change.
- RFC-038's `OpenAnyway` and `ShowInFolder` recovery actions call the same two
  functions. That un-holds them from `HANDOFF-038`; say so in its submission.

Copy goes through the typed catalog in both locales and must pass the
forbidden-terms gate. Proposed, for the owner to confirm:

| | EN | JA |
|---|---|---|
| Open file | Open file | ファイルを開く |
| Show in folder | Show in folder | フォルダで表示 |

## 4. Tests

1. **Validated, then launched:** selecting a result and pressing Open file
   calls the fake launcher once, with that result's canonical path.
2. **Refused outside every source:** a result whose file lies outside every
   registered source, or whose source is paused, does **not** reach the
   launcher. Mutation: bypass the guard and watch this test fail.
3. **Keyboard:** Enter with a result selected and focus outside a text input
   sends the open message; with focus in the search box it does not.
4. **No shell:** a test, or a cited line from the dependency's source, showing
   how the process is spawned on Linux.

## 5. Definition of done

- A found document can be opened and revealed from the UI, on all three
  platforms, through the guard.
- No test launches a real application.
- The dependency decision is reported with its graph impact.
- CHANGELOG entry under `[Unreleased]`.

## 6. Stop conditions

- The only cross-platform option spawns through a shell. Report it; do not
  ship it.
- Reveal-in-folder adds a D-Bus or similarly heavy stack on Linux. Ship open,
  report reveal.
- Binding Enter changes an existing context's behaviour in `a11y.rs`.
