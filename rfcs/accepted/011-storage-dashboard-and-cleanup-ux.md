# RFC-011: Storage Dashboard and Cleanup UX

**Project:** orbok  
**RFC:** 011  
**Title:** Storage Dashboard and Cleanup UX  
**Status:** Accepted
**Target Milestone:** M10  
**Date:** 2026-06-06  

**Returned to `accepted/` 2026-09-22 (Task 083, Review Request 259 §6).**
Carried `Implemented (v0.4.0)` while §14 criteria **5 and 6** are false:
`DeleteKeywordIndex`/`DeleteVectorIndex` have no executor arm (the only
`CleanupExecutor` methods are `run_safe` and `run_reset_catalog`; routing
either action through `run_safe` returns
`Err(CleanupWouldTouchPersistentData)`, a plan-shaped answer for an
action the code never actually implements) and no caller anywhere in
`crates/app` — deleting the keyword or semantic index independently is
unreachable from the product, so nothing ever marks a rebuild required.
**Criterion 7 held as of Task 086 (2026-09-23, §9a):** reset catalog's
confirmation (Task 062's dialog: Escape cancels, Enter confirms while
visible, Task 069 closes it on view change) is Cancel/Confirm, not the
typed `Type RESET to confirm` §9 originally named — the owner decided the
RFC changes to describe that dialog, not that the product grows a typing
field. Criteria 2, 3, 4, 7 and 9 hold, each with an end-to-end test;
criteria 1, 8 and 10 were evidenced by Task 081. No closure record: see
`rfcs/closures/LEGACY-ALLOWLIST.txt` (`011` stays listed; removing it
means writing the record, and there is no record until 5 and 6 are true or
the RFC is amended to drop them).

---

## 1. Summary

This RFC defines the Storage Dashboard and cleanup UX for `orbok`.

The central decision is:

> Storage management must be lifecycle-aware. The UI must distinguish persistent catalog data, rebuildable index data, ephemeral cache data, local model files, and logs before any cleanup action is allowed.

This RFC turns the data lifecycle model from RFC-001 into concrete product behavior.

---

## 2. Motivation

`orbok` is storage-conscious by design. It avoids duplicating source files, but it still stores derived data:

- file catalog records;
- chunk metadata;
- keyword indexes;
- embeddings;
- vector index files;
- snippets;
- search caches;
- model files;
- `localcache` payloads.

If the UI simply says “clear cache” or “delete data,” users may misunderstand what is safe to remove, what requires rebuilding, and what affects search quality.

Storage transparency is central to user trust.

---

## 3. Goals

- Show how much local storage `orbok` uses.
- Break storage down by meaningful lifecycle categories.
- Provide safe cleanup actions.
- Provide rebuildable-index cleanup actions with clear warnings.
- Prevent accidental deletion of persistent source configuration.
- Integrate `localcache` storage accounting.
- Explain that source files are never deleted by cleanup.
- Provide confirmation levels appropriate to impact.

---

## 4. Non-Goals

- This RFC does not define backup/export features.
- This RFC does not implement vector compression.
- This RFC does not define OS-level disk cleanup.
- This RFC does not delete source files.
- This RFC does not provide secure deletion guarantees.

---

## 5. Storage Categories

The Storage Dashboard must display at least:

| Category | Lifecycle Class | Examples |
|---|---|---|
| Persistent catalog | Persistent catalog | sources, file records, settings, model registry |
| Exact search index | Rebuildable index | keyword/FTS/Tantivy index |
| Semantic search index | Rebuildable index | embeddings, vector segments |
| Temporary extraction cache | Rebuildable/ephemeral | localcache extracted segment payloads |
| Snippet cache | Ephemeral cache | preview snippets |
| Search cache | Ephemeral cache | query result cache |
| Model files | Local dependency | embedding/reranker models |
| Logs and diagnostics | Operational data | redacted app logs |

---

## 6. Storage Dashboard Layout

Recommended desktop layout:

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Storage                                                                      │
│ See what orbok stores and clean up safely.                                   │
├──────────────────────────────────────────────────────────────────────────────┤
│ Total orbok storage: 1.42 GB                                                  │
│                                                                              │
│ ┌───────────────────────────┐ ┌────────────────────────────────────────────┐ │
│ │ Storage Breakdown          │ │ Cleanup Actions                            │ │
│ │                           │ │                                            │ │
│ │ Persistent catalog  24 MB │ │ Safe cleanup                               │ │
│ │ Exact index        180 MB │ │ [Clear expired search cache]                │ │
│ │ Semantic index     920 MB │ │ [Clear temporary snippets]                  │ │
│ │ Extraction cache    32 MB │ │ [Remove replaced stale indexes]             │ │
│ │ Search cache        12 MB │ │                                            │ │
│ │ Models             260 MB │ │ Space recovery                             │ │
│ │ Logs                2 MB  │ │ [Delete semantic index and rebuild later]   │ │
│ └───────────────────────────┘ │ [Delete exact index and rebuild later]      │ │
│                               │                                            │ │
│                               │ Dangerous                                  │ │
│                               │ [Reset orbok catalog...]                   │ │
│                               └────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 7. Cleanup Action Classes

## 7.1. Safe Cleanup

Safe cleanup removes only data that is not required for correctness.

Examples:

- expired search cache;
- expired snippet cache;
- stale replaced index fragments;
- temporary extraction buffers;
- old rerank cache.

No strong confirmation required, but the action should still be visible.

## 7.2. Space Recovery Cleanup

Space recovery cleanup deletes rebuildable indexes.

Examples:

- delete semantic search index;
- delete exact search index;
- delete temporary source indexes;
- delete localcache chunk/embedding bundles.

Requires confirmation.

The UI must say:

```text
This will not delete your source files.
Search may be slower or incomplete until the index is rebuilt.
```

## 7.3. Destructive Reset

Destructive reset deletes persistent catalog data.

Examples:

- reset orbok catalog;
- remove all registered sources;
- remove source policies;
- remove file catalog.

Requires strong confirmation.

---

## 8. CleanupPlan API

All cleanup actions must first create a cleanup plan.

Conceptual model:

```rust
pub struct CleanupPlan {
    pub action: CleanupAction,
    pub affected_categories: Vec<StorageCategory>,
    pub affected_lifecycle_classes: Vec<DataClass>,
    pub estimated_recovered_bytes: u64,
    pub deletes_source_files: bool,
    pub requires_rebuild: bool,
    pub requires_confirmation: ConfirmationLevel,
    pub warnings: Vec<String>,
}
```

Rules:

- `deletes_source_files` must always be false for normal orbok cleanup.
- UI must display plan before high-risk actions.
- Backend must execute only plans it created.

---

## 9. Confirmation Levels

| Level | Use |
|---|---|
| none | expired cache cleanup |
| normal | remove temporary source index |
| strong | delete exact/semantic index |
| typed | reset catalog |

Typed confirmation example:

```text
Type RESET to confirm.
```

## 9a. Amendment 1 (2026-09-23) — what orbok's reset confirmation actually is

Task 086's origin: the product never built the typed-word confirmation
above. Reset catalog asks with Task 062's dialog instead — a title, the
warning, a Cancel button and a danger button — and the owner decided the
RFC changes, not the product. No typing field is built.

**Why the dialog is the better design here, not merely what shipped:**

- **Reset does not destroy the user's files.** It removes what orbok
  prepared, which orbok can prepare again. A typed word is the convention
  for irreversible loss of the user's own data, which this is not.
- **It is keyboard-operable without a text field.** iced 0.14 buttons
  cannot take focus (RFC-034 §5.3's Task 062 amendment, citing §5.4), so a
  typed-word field would be the only focusable control in the dialog, and
  `Enter` inside it would have to be bound to confirm — the same key the
  dialog already uses, with an extra step that teaches nothing.
- **The protection is that it cannot be confirmed unseen** (Task 069:
  switching views closes the dialog, so a stale `Enter` cannot land on it),
  and that the danger button is styled and labelled as destructive
  (RFC-033 §6).

**The dialog, precisely:**

- The reset confirmation renders a title, the warning text, a Cancel
  button, and a danger button labelled the same as the title — each
  button sends the message it names
  (`the_reset_confirmation_renders_cancel_and_the_danger_button`,
  `crates/ui/src/tests/task062_reset_catalog_confirmation.rs`).
- `Escape` cancels it and nothing is reset — the only route to an actual
  reset is `Message::ConfirmResetCatalog` reaching the backend, and
  `Escape` never sends it
  (`escape_cancels_the_reset_confirmation_and_resets_nothing`, same file).
- `Enter` confirms it only while it is the one thing on screen — not from
  another view, and not under an open wizard
  (`crates/ui/src/tests/task069_confirm_only_what_is_on_screen.rs`:
  `on_its_own_view_each_confirmation_is_confirmed_by_enter`,
  `switching_view_closes_every_confirmation_and_enter_cannot_confirm_it`,
  `a_wizard_over_an_open_confirmation_never_confirms_it`).
- A failed reset leaves the list exactly as the catalog holds it, not a
  blind clear and not the state from before the attempt
  (`crates/app/src/backend_actions/tests.rs`:
  `a_reset_that_fails_under_a_write_lock_leaves_the_list_as_the_catalog_holds_it`,
  `a_reset_whose_reload_also_fails_leaves_the_list_as_it_was`,
  `a_reset_that_fails_after_the_catalog_step_shows_what_the_catalog_holds`).

**§14 criterion 7 ("Reset catalog requires strong typed confirmation") is
met by this dialog**, not by the typed word above: the criterion's
substance — a deliberate, non-accidental, clearly-labelled confirmation
before an irreversible-feeling action — holds under the Cancel/danger
design for the reasons stated above. The criterion's own wording is not
edited here, since it is the acceptance table's record of what was asked
for; the amendment is what settles that asking for a typed word was not
itself the requirement.

---

## 10. localcache Integration

`localcache` namespaces must map into storage categories.

Recommended mapping:

| localcache Namespace | Storage Category |
|---|---|
| `extract-segments:*` | Temporary extraction cache |
| `normalized-text:*` | Temporary extraction cache |
| `chunk-bundle:*` | Rebuildable index |
| `embedding-bundle:*` | Semantic search index |
| `preview-cache:*` | Snippet cache |

The Storage Manager must query `localcache` stats through the `orbok` cache service wrapper, not directly from UI code.

Cleanup must call:

- expired cleanup;
- stale-version cleanup;
- missing-file cleanup;
- namespace deletion;
- database shrink where appropriate.

---

## 11. Storage Accounting

`storage_accounting` table should be updated by:

- scheduled recalculation;
- after cleanup actions;
- after index build/rebuild;
- after model install/remove;
- after localcache cleanup.

Categories:

```text
persistent_catalog
keyword_index
vector_index
temporary_extraction
snippet_cache
search_cache
model_files
logs
```

---

## 12. UI Copy Requirements

Use clear wording:

```text
Source files will not be deleted.
This data can be rebuilt from your source files.
Semantic search may be unavailable until rebuilding completes.
This action removes registered source settings.
```

Avoid vague labels:

```text
Delete data
Clear database
Remove files
Clean everything
```

---

## 13. Empty and Error States

## 13.1. Storage Accounting Unknown

```text
Storage usage has not been calculated yet.
[Calculate Now]
```

## 13.2. Cache Database Missing

```text
The cache database is missing or was removed.
orbok can recreate it automatically.
[Recreate Cache]
```

## 13.3. Cache Database Corrupt

```text
The cache database appears to be damaged.
You can rebuild cache data from source files.
[Rebuild Cache]
```

---

## 14. Acceptance Criteria

- Storage Dashboard shows all required categories.
- Safe cleanup never deletes persistent catalog data.
- Cleanup plan is generated before cleanup execution.
- Source files are never deleted by cleanup.
- Deleting semantic index marks rebuild required.
- Deleting exact index marks rebuild required.
- Reset catalog requires strong typed confirmation.
- `localcache` stats appear in storage accounting.
- Text-bearing caches can be deleted.
- Model files are shown separately from indexes.

---

## 15. Testing Requirements

Required tests:

1. Safe cleanup preserves sources.
2. Safe cleanup removes expired snippet cache.
3. Delete semantic index preserves file catalog.
4. Delete exact index preserves source settings.
5. Reset catalog removes sources only after strong confirmation.
6. Cleanup plan reports no source-file deletion.
7. localcache namespace size is included.
8. Corrupt cache database can be rebuilt.
9. Privacy-strict mode clears text-bearing caches.
10. UI copy distinguishes source files from orbok indexes.

---

## 16. Unresolved Questions

- Should storage accounting be exact or approximate by default?
- Should localcache database shrink run automatically?
- Should model files be removable from Storage or only Models view?
- Should vector compression be exposed here or in advanced settings?
- Should backup/export include rebuildable indexes?

---

## 17. Decision

Implement a lifecycle-aware Storage Dashboard before release.

All cleanup must be mediated by a backend-generated `CleanupPlan`.
