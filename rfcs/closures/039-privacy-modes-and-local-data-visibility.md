# Closure Record — RFC-039: Privacy Modes and Local Data Visibility

**RFC:** [039](../done/039-privacy-modes-and-local-data-visibility.md)
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** the privacy work that shipped in v0.19.0, and Task 115
(Amendment 1, §20a: privacy is the fixed defaults and one switch). Task 115 is
not git-tracked (RFC-063 §5): `.git-exclude/tasks/dev-team/115-…`; the audit it
acts on is Review Request 290 (`.git-exclude/review-request/290-…`) and Review
290. **Criteria are read as Amendment 1 words them** (§20a lists which changed:
1, 3, 4, 5, 9). Transcribed from what was run on 2026-09-25.

---

## §20 acceptance criteria

### 1. Privacy modes are defined.

→ **as amended:** the privacy defaults are defined once, in
`PrivacySettings::default` (`crates/core/src/privacy.rs`); there is no mode.
→ what was run: `recent_searches_default_on_and_follow_the_toggle_only`
(`crates/core/src/tests.rs`); `any_other_privacy_mode_or_none_changes_nothing`
(`crates/app/src/settings/tests.rs`).
→ what was observed: the defaults are on (recent searches) and off (every
diagnostics inclusion); a `settings.json` carrying any leftover
`privacy_mode` value other than `"strict"` changes nothing, and the field is
never written again.

### 2. Standard mode is useful by default.

→ what was run: `recent_searches_follow_the_toggle_and_nothing_else`
(`settings/tests.rs`).
→ what was observed: with nothing set, recent searches are recorded (the
toggle's default is On); the defaults work out of the box.

### 3. Strict mode reduces remembered data.

→ **as amended:** there is no Strict mode. What reduces remembered data is the
toggle (Off) and Storage's cleanup; a leftover `"strict"` is honoured once.
→ what was run: `a_strict_privacy_mode_loads_as_recent_searches_off_and_is_not_saved_again`,
`recent_searches_follow_the_toggle_and_nothing_else` (`settings/tests.rs`).
→ what was observed: `"strict"` loads as the toggle Off; the saved file no
longer carries `privacy_mode` and stays Off on the next load; with the toggle
Off no search is recorded. Dropping the compatibility rule fails both tests.

### 4. Recent searches obey privacy mode.

→ **as amended:** they obey the toggle, and nothing else.
→ what was run: `recent_searches_follow_the_toggle_and_nothing_else`; the
visible control is Settings → Privacy → **Remember recent searches**.
→ what was observed: across `standard`, `portable`, `diagnostics` and absent
`privacy_mode` values, the toggle On records one search and Off records none.

### 5. Diagnostics obey privacy mode.

→ **as amended:** diagnostics have no mode to obey; the policy's defaults are
fixed, and RFC-040 (accepted, unbuilt) owns the rest.
→ what was run: `diagnostics_policy_never_enables_raw_paths_by_default`
(`crates/core/src/tests.rs`); `bundle_preview_text` (`crates/app/src/diagnostics.rs`).
→ what was observed: raw paths are never included by default, sensitive
opt-ins are offered at the Standard default. **No bundle can be created from
the app** until RFC-040 is built; this is stated in the amendment, not hidden.

### 6. Temporary previews have clear cleanup behavior.

→ what was run: `cleanup_service_safe_preserves_sources`
(`crates/pipeline/workers/src/tests/v06_features.rs`) and Task 081's Storage
tests.
→ what was observed: Storage → Safe cleanup → **Clear temporary previews**
removes them and keeps the folder list; the category line "Temporary previews"
is shown. The controls this RFC named (`PrivacyTemporaryPreviews`,
`PrivacyClearPreviews`) were never built and their keys are deleted; Storage's
own copy does the job.

### 7. Model download copy is accurate.

→ what was run: the glossary and the unused-key test
(`crates/ui/src/tests/glossary.rs`, `task109_every_key_is_shown.rs`); the
consent screen's labels (`ModelConsentRevision` "Version", `ModelArtifactTokenizer`
"Vocabulary").
→ what was observed: the consent screen shows the provider, the exact size and
the license, and the wizard says "No files are uploaded — inference runs
locally." `PrivacyModelDownloadNote` is deleted (superseded by that copy).

### 8. User files are never deleted by cleanup without explicit destructive reset.

→ what was run: as `rfcs/closures/011-storage-dashboard-and-cleanup-ux.md`
criteria 2 and 4; the reset and removal dialogs.
→ what was observed: they say "Your files are never changed or deleted." and
no cleanup path touches the user's files.

### 9. Default UI uses plain privacy language.

→ **as amended:** the default UI's privacy language is one sentence and one
toggle.
→ what was run: the glossary's default-copy scan
(`crates/ui/src/tests/glossary.rs`).
→ what was observed: Settings → Privacy shows "Documents are processed on this
computer only.", **Remember recent searches** and its note; the words
Standard, Strict and Portable appear nowhere in the app (their keys are deleted).

### 10. Tests verify sensitive defaults.

→ what was run: `safe_policy_defaults`, `recent_searches_default_on_and_follow_the_toggle_only`,
`diagnostics_policy_never_enables_raw_paths_by_default` (`crates/core/src/tests.rs`).
→ what was observed: every diagnostics inclusion defaults to off; recent
searches default to on and follow only the toggle.

---

## Criteria not met, and why RFC-039 closes anyway

None, as Amendment 1 words the criteria. What the original text promised and
the product does not offer is named in §20a: the mode picker, Strict, Portable
and Diagnostics modes, and the snippet-persistence setting. Diagnostics export
waits for RFC-040.
