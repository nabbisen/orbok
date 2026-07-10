# RFC-027: GUI Framework Finalization

**Project:** orbok
**RFC:** 027
**Title:** GUI Framework Finalization
**Status:** Implemented (v0.1.0)
**Target Milestone:** M0 (shell), M9 (Search UI MVP)
**Date:** 2026-06-06
**History:** Originally a deferred future RFC. Activated 2026-06-06 by
project-owner decision mandating `snora` v0.8 as the GUI framework.

---

## 1. Summary

This RFC finalizes the GUI framework for `orbok`.

The decision is:

> `orbok` uses a **native Rust GUI** built on **iced 0.14** through the
> **`snora` v0.8** framework (`snora` / `snora-widgets` / `snora-core`).
> There is no WebView, no embedded browser, and no local HTTP API in v1.

The earlier design documents (external design §4.2, GUI/UX design §0,
RFC-015) kept three options open: Tauri + Svelte 5, a local loopback web
UI, or a native Rust GUI. The project owner has resolved this question in
favor of the native Rust option, with `snora` as the application skeleton.

This RFC records the decision, its rationale, and its consequences for
the other RFCs.

---

## 2. Rationale

### 2.1. Fit

`snora`'s own fit guidance describes `orbok` almost verbatim: a
local-first desktop app that runs heavy work (AI inference, file
processing) alongside an interactive UI, with standard desktop chrome
(header / sidebar / body / footer) and a small set of overlays. The
`orbok` GUI/UX external design (§4 Global Application Shell) maps 1-to-1
onto `snora`'s `AppLayout` slots.

### 2.2. Security surface

A native iced GUI removes the two largest attack-surface concerns in
RFC-015:

- **No WebView.** No untrusted HTML rendering engine is embedded;
  extracted document text is rendered as plain iced text widgets, which
  cannot execute scripts or load remote resources by construction.
- **No local HTTP API.** The frontend/backend boundary is an in-process
  Rust API, not a loopback socket. Cross-origin request forgery against
  a local API is structurally impossible because no socket exists.

The RFC-003 boundary rule ("the frontend never receives unrestricted
file-system access") is preserved in a new form: the `orbok-ui` crate
**must not** perform direct file-system access (`std::fs`, `tokio::fs`,
etc.). All file operations go through `orbok-core` service interfaces,
which enforce source-membership validation exactly as RFC-003 requires.
This rule is enforceable by code review and a lint/CI grep, and keeps the
boundary meaningful even without a process boundary.

### 2.3. Privacy story

"Documents stay local" is easier to demonstrate when the application
contains no web runtime at all. Network access in v1 is limited to the
(future, explicit) model-installation workflow of RFC-012/RFC-029.

### 2.4. Packaging

One static-ish native binary per platform (plus assets). No Node/npm
toolchain, no WebView runtime version matrix. This simplifies RFC-017
considerably. iced 0.14 supports Linux, Windows, and macOS, matching
NFR-050.

### 2.5. Accessibility and i18n

- `snora` exposes logical edges (`Edge::Start`/`End`) and RTL layout as
  first-class API, satisfying part of GUI/UX design §17.
- iced provides keyboard events and focus handling for the §17.1
  shortcut requirements.
- iced's text stack (cosmic-text) shapes CJK text, which matters for the
  Japanese-language requirements of RFC-014 and the i18n requirement of
  RFC-031.

### 2.6. Costs and accepted risks

| Risk | Assessment |
|---|---|
| iced API churn between releases | Accepted; `snora` absorbs part of it, and the UI crate is isolated behind `orbok-core` service interfaces |
| Slower UI iteration than web stacks | Accepted; the GUI/UX design is already fully specified, reducing exploration cost |
| Rich-text/preview rendering is more manual | Accepted; RFC-013 preview shows escaped plain text by design (RFC-015 §safe rendering) |
| Accessibility tree exposure (screen readers) is weaker in iced than in native/web stacks today | Tracked; revisit when iced's a11y integration matures. Non-color status badges, focus visibility, and keyboard navigation are implemented now |
| Headless CI cannot run the GUI | UI logic is kept in pure functions over view-model structs; CI builds the UI crate and unit-tests view-models without a display |

---

## 3. Architecture Impact

### 3.1. Process model

```text
single process
├── orbok-ui (iced + snora)          ← view layer, no file access
│      │ iced Messages / Tasks
├── orbok-core services              ← application boundary (was "/api/*")
│      │
├── orbok-fs / orbok-extract / orbok-search / orbok-models
├── orbok-db    (catalog: orbok-catalog.sqlite3)
└── orbok-cache (localcache: orbok-cache.sqlite3)
```

The conceptual REST groups of external design §8 (`/api/search/*`,
`/api/sources/*`, …) map to Rust service traits with the same
responsibility split. Long-running work (scan, extract, index) runs on
background tasks; the UI receives progress via iced subscription/Task
messages, fulfilling the "scanner must not block UI" requirement of
RFC-004 §15.

### 3.2. UI component mapping

| GUI/UX design component (§18) | snora / iced realization |
|---|---|
| AppShell | `AppLayout::new(body).header(h).side_bar(s).footer(f)` + `render` |
| TopBar | `snora::widget::app_header` slot content |
| SidebarNav | `SideBar` + `SideBarItem` with `ViewId` enum |
| StatusBadge | text badge widgets (non-color-only, per §17.4) |
| Add/Remove/Cleanup dialogs | `snora` `Dialog` overlay + `on_close_modals` |
| Filter drawer | `snora` `Sheet` (`SheetEdge::End`) |
| Toasts / notifications | `snora` `Toast` lifecycle helpers |
| Breadcrumb (heading path) | `app_breadcrumb` |

### 3.3. View structure

One `ViewId` per top-level page (Search, Sources, Indexing, Storage,
Models, Settings), each view a plain function over a view-model struct,
per the `snora` `multi_view` pattern. View-model structs live in
`orbok-ui` and contain only display data — no repository or path types.

---

## 4. Consequences for Other RFCs

| RFC | Consequence |
|---|---|
| RFC-003 | "Frontend" boundary reinterpreted as the `orbok-ui` crate boundary; backend validation rules unchanged |
| RFC-013 | Layouts implemented with iced widgets; semantics unchanged |
| RFC-015 | §local API hardening (loopback binding, CSRF token, origin checks) becomes **not applicable in v1**; recorded as an amendment in that RFC. WebView-specific rules likewise dormant. All file-boundary, log-hygiene, and safe-rendering rules remain in force |
| RFC-017 | Packaging targets a native binary; no WebView runtime dependency |
| RFC-031 | i18n catalog lives in `orbok-ui`; see RFC-031 |

---

## 5. Dependency Policy

```toml
iced  = { version = "0.14", features = ["tokio"] }
snora = "0.8"
```

`snora` v0.8.0 (workspace: `snora`, `snora-widgets`, `snora-core`) is the
pinned framework. Questions and improvement requests go upstream to the
crate author (the project has a direct channel to the `snora` and
`localcache` author).

Rust edition 2024 is required by `snora` and matches the project
standard.

---

## 6. Acceptance Criteria

- App shell renders header, sidebar with six views, body, and footer.
- View switching works via sidebar.
- `orbok-ui` contains no direct file-system access (CI-checkable).
- Dialog and toast overlays function for at least one workflow each.
- The packaged binary starts on Linux, Windows, and macOS.
- Keyboard navigation works for primary actions.
- UI strings flow through the RFC-031 i18n catalog.

---

## 7. Testing Strategy

- View-model construction and message-update logic unit-tested headlessly.
- `orbok-ui` compiles in CI without a display server.
- Manual smoke matrix per platform for shell, overlays, and focus order
  (RFC-019 release gate).

---

## 8. Decision

Adopt **iced 0.14 via snora v0.8** as the production GUI framework.
No WebView and no local HTTP API in v1. The `orbok-ui` crate is the
frontend boundary and performs no direct file access.
