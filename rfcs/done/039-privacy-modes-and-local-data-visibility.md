# RFC-039: Privacy Modes and Local Data Visibility

**Project:** orbok  
**Former project name:** orbit  
**RFC:** 039  
**Title:** Privacy Modes and Local Data Visibility  
**Status:** Implemented (v0.19.0)
**Amended:** 2026-09-25 (Amendment 1, §20a: privacy is the fixed defaults and one switch)
**Target milestone:** Privacy UX / local-first trust  
**Date:** 2026-06-18  
**Related RFCs:** RFC-001 Local Data Classification and Lifecycle, RFC-011 Storage Dashboard and Cleanup UX, RFC-042 Search History and Reopen Recent Searches, RFC-043 Model Download Readiness Check and Bounded Concurrency, RFC-040 Safe Diagnostics and Redacted Support Bundle  

---

## 1. Summary

This RFC defines a unified privacy model for orbok.

orbok’s core promise is:

```text
Documents stay on this computer.
```

But local-only does not automatically mean privacy-safe. The app may still store sensitive local data such as recent searches, extracted text, snippets, temporary previews, paths, logs, diagnostics, model files, and failure records.

This RFC defines privacy modes and data visibility rules so orbok behaves consistently.

---

## 2. Motivation

Several features touch privacy: Recent searches, search result snippets, temporary previews, extraction cache, logs, diagnostics bundle, model download, portable mode, and strict privacy expectations.

Without a unified privacy model, features may make inconsistent decisions.

Example risk:

```text
Search history is disabled,
but diagnostics still exports search text.
```

This RFC prevents that.

---

## 3. Goals

- Define privacy modes.
- Define which local data is stored in each mode.
- Define cleanup behavior.
- Define diagnostics behavior.
- Keep user copy plain.
- Preserve useful defaults for ordinary users.
- Provide strict mode for sensitive work.
- Make portable mode predictable.
- Avoid hidden sensitive persistence.

---

## 4. Non-Goals

This RFC does not define full disk encryption, OS account security, cloud sync, enterprise policy, remote telemetry, secure deletion guarantees, cryptographic key management, or encrypted local indexes.

Encrypted local indexes are future work and should not be implied by this RFC.

---

## 5. Privacy Modes

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyMode {
    Standard,
    Strict,
    Portable,
    Diagnostics,
}
```

### 5.1. Standard

Default mode.

Copy:

```text
Documents are processed on this computer only.
```

Behavior:

- recent searches on by default;
- temporary previews allowed;
- extracted/indexed data allowed;
- logs redacted by default;
- diagnostics redacted by default.

### 5.2. Strict

For sensitive environments.

Copy:

```text
Strict privacy reduces what orbok remembers.
```

Behavior:

- recent searches off;
- temporary previews minimized;
- search result cache off or shortened;
- logs highly redacted;
- diagnostics excludes sensitive fields;
- cleanup prompts offered when enabling.

### 5.3. Portable

For `--portable` mode or explicit portable data directory.

Copy:

```text
orbok stores app data next to this copy of the app.
```

Behavior:

- same as Standard unless user chooses Strict;
- data location is clearly shown;
- easy “open data folder” action;
- warnings if portable location is removable.

### 5.4. Diagnostics

Temporary opt-in mode for troubleshooting.

Copy:

```text
Include extra details for troubleshooting.
```

Behavior:

- must be explicitly enabled;
- time-limited or one-export only;
- still excludes document contents by default;
- sensitive inclusions require separate checkboxes.

---

## 6. Data Categories

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalDataCategory {
    SourcePaths,
    FileMetadata,
    ExtractedText,
    KeywordIndex,
    Embeddings,
    Snippets,
    TemporaryPreviews,
    RecentSearches,
    Logs,
    Diagnostics,
    ModelFiles,
    Settings,
}
```

---

## 7. Data Visibility Matrix

| Data | Standard | Strict | Portable | Diagnostics |
|---|---|---|---|---|
| Source paths | stored | stored | stored in portable dir | redacted by default |
| File metadata | stored | stored | stored | aggregate only by default |
| Extracted text | rebuildable storage allowed | minimized where practical | allowed | never exported by default |
| Keyword index | stored | stored unless user disables | stored | not exported by default |
| Embeddings | stored | stored unless user disables | stored | not exported by default |
| Snippets | allowed | minimized/short TTL | allowed | not exported by default |
| Temporary previews | allowed | minimized/clear on exit optional | allowed | not exported |
| Recent searches | on by default | off | on unless strict | excluded by default |
| Logs | redacted | strongly redacted | redacted | included redacted |
| Diagnostics | redacted export | redacted export | redacted export | expanded with opt-in |
| Model files | stored | stored | stored | status only |
| Settings | stored | stored | stored | safe subset |

---

## 8. Settings UI

```text
Privacy

Documents are processed on this computer only.

Privacy mode
[Standard ▾]

Remember recent searches
[On]
Recent searches are saved on this computer only.

Temporary previews
[Clear temporary previews]

Diagnostics
[Create support file]
```

Strict mode:

```text
Privacy mode
[Strict]

Recent searches are not saved.
Temporary previews are reduced.
Support files hide sensitive details by default.
```

---

## 9. Enabling Strict Mode

When user turns on Strict:

```text
Turn on Strict privacy?

orbok will stop saving recent searches and reduce temporary previews.
You can also clear data already saved.

[Cancel] [Turn on] [Turn on and clear]
```

If “turn on and clear”:

- clear recent searches;
- clear temporary previews;
- clear search result cache;
- keep source folders unless explicitly reset;
- keep indexes unless user chooses deeper cleanup.

Do not delete user files.

---

## 10. Recent Searches Policy

From RFC-042:

Standard:

```text
Remember recent searches: On
```

Strict:

```text
Remember recent searches: Off
```

If strict is enabled:

```text
Recent searches are not saved while Strict privacy is on.
```

---

## 11. Temporary Previews and Snippets

Standard:

- may store short-lived snippets/previews;
- user can clear them.

Strict:

- avoid persistent snippet cache where practical;
- clear on exit option may be enabled;
- previews generated from source on demand if possible.

User copy:

```text
Temporary previews help results open faster. You can clear them anytime.
```

---

## 12. Logs

Default logging must not include:

- document contents;
- search text;
- raw snippets;
- full paths unless necessary and redacted;
- model URLs with sensitive query tokens.

Use stable IDs or redacted paths.

Raw path:

```text
/home/user/Documents/secret/report.pdf
```

Redacted path:

```text
<folder>/report.pdf
```

---

## 13. Model Download Privacy

Model download contacts the model host.

The app must say:

```text
orbok downloads the search helper, but your documents are not uploaded.
```

Do not imply the app is fully offline during model download. After model files are installed, search runs locally.

---

## 14. Diagnostics Interaction

Diagnostics export must follow RFC-040.

Defaults:

- no recent searches;
- no snippets;
- no document text;
- no raw paths;
- no full index data;
- no embeddings.

Opt-ins must be separate and clear.

---

## 15. Storage Dashboard Interaction

Storage dashboard should show:

```text
Search data
Search helper
Temporary previews
Recent searches
Logs
```

Avoid:

```text
cache
catalog
vector index
embedding store
```

unless Advanced view is on.

---

## 16. Data Cleanup Levels

### 16.1. Safe Cleanup

Does not affect configured folders or core search data.

Examples:

- clear temporary previews;
- clear old search results;
- clear recent searches.

### 16.2. Rebuild Cleanup

Search data will be rebuilt.

Examples:

- rebuild search data;
- clear prepared data for a folder.

### 16.3. Reset

Forgets app state.

Examples:

- forget folders;
- reset saved app data.

Always say:

```text
Your files will not be deleted.
```

---

## 17. Data Model

```rust
pub struct PrivacySettings {
    pub mode: PrivacyMode,
    pub remember_recent_searches: bool,
    pub persist_snippets: bool,
    pub clear_temporary_previews_on_exit: bool,
    pub diagnostics_include_paths: bool,
    pub diagnostics_include_recent_searches: bool,
}
```

Rules:

- strict mode can force some settings off;
- UI should show when a setting is controlled by strict mode.

---

## 18. Events

```rust
pub enum PrivacyEvent {
    ModeChanged(PrivacyMode),
    RecentSearchesDisabled,
    TemporaryPreviewsCleared,
    StrictModeCleanupRequested,
    DiagnosticsExportRequested,
}
```

---

## 19. Testing

### 19.1. Unit Tests

- strict disables recent searches;
- strict disables or shortens snippet persistence;
- standard defaults recent searches on;
- portable data location reported correctly;
- diagnostics flags default false.

### 19.2. Integration Tests

- enabling strict clears selected data when requested;
- search works with history disabled;
- snippets do not persist in strict mode if configured;
- diagnostics excludes sensitive fields.

### 19.3. Copy Tests

- no “cache/catalog/vector” labels in default privacy UI;
- local-only copy is accurate;
- model download copy does not imply zero network.

---

## 20. Acceptance Criteria

This RFC is accepted when:

1. Privacy modes are defined.
2. Standard mode is useful by default.
3. Strict mode reduces remembered data.
4. Recent searches obey privacy mode.
5. Diagnostics obey privacy mode.
6. Temporary previews have clear cleanup behavior.
7. Model download copy is accurate.
8. User files are never deleted by cleanup without explicit destructive reset.
9. Default UI uses plain privacy language.
10. Tests verify sensitive defaults.

## 20a. Amendment 1 (2026-09-25) — privacy is the fixed defaults and one switch

Task 115's origin: Task 112's audit (Review Request 290) found that the
modes this RFC defines were never a product. `PrivacyMode` existed in
`orbok-core` and in `settings.json`, no screen named or set a mode, and its one
reachable effect (Strict turning recent searches off) needed a hand-edited
file. A mode that no screen can set, and whose only effect is reachable that
way, is worse than no mode: a user who set `"strict"` by hand got a Settings
page whose **Remember recent searches** toggle might not describe what orbok
did. The owner decided (2026-09-24, fixed safe defaults and few settings; and
Review 290 §1) that the RFC changes, not the product.

**What is true now.**

- **Privacy is fixed safe defaults:** documents are processed on this computer
  only; nothing is uploaded; files are never changed or deleted by cleanup.
- **The one control is Remember recent searches** (Settings → Privacy). It is
  the whole truth about whether a search is recorded.
- **Text-bearing caches are cleared from Storage** (Safe cleanup, Task 081
  and after), not from a privacy screen.
- **Diagnostics wait for RFC-040** (accepted, unbuilt). Its policy type and
  messages stay; with no mode to read, they behave as the Standard mode
  always did (sensitive opt-ins may be offered, none is on).
- **`PrivacyMode`, `settings.privacy_mode`, `PrivacySettings.mode`,
  `persist_snippets`, `allows_snippet_persistence` and the three messages no
  view sent (`SetPrivacyMode`, `PrivacySettingChanged`,
  `ClearTemporaryPreviews`) are removed.** The 19 keys that named the mode
  screen (§8, §9, §10, §11) are deleted.
- **Compatibility.** A `settings.json` that says `"strict"` loads as
  **Remember recent searches: Off**, once, and the next save no longer
  writes `privacy_mode`: a user's effective choice is never silently
  reversed. Any other value, or none, changes nothing.

**Sections superseded:** §5 (the four modes; Standard is simply the defaults),
§8 (the mode picker), §9 (enabling Strict), §10's "Strict disables history",
§11's snippet-persistence setting, and §14's mode input to diagnostics.
§13 (model download privacy), §15–§16 (Storage and cleanup) stand.

**Criteria changed (§20).** **1** — modes are not a product concept; the
criterion is met by the fixed defaults being defined in one place
(`PrivacySettings::default`). **3** — there is no Strict mode; what reduces
remembered data is the toggle (Off) and Storage's cleanup. **5** — diagnostics
have no mode to obey; the policy's defaults are fixed and RFC-040 owns the
rest. **9** — the default UI's privacy language is one sentence and one
toggle, in plain words. **4** now reads "recent searches obey the toggle".
Criteria 2, 6, 7, 8 and 10 are unchanged.

---

## 21. Final Decision

Implement unified privacy modes:

```text
Standard
Strict
Portable
Diagnostics
```

Use these modes to govern recent searches, previews, logs, diagnostics, and visible data cleanup.
