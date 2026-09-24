# Closure Record — RFC-043: Model Download Readiness Check and Bounded Concurrency

**RFC:** [043](../done/043-model-download-readiness-and-concurrency.md)
**Format:** RFC-063 §6.1/§6.2 option B.
**Implemented by:** the download work that shipped in v0.19.0, rebuilt by
RFC-050 (the managed model store), and Task 115 (Amendment 1, §26a: the copy
was superseded by RFC-050's wizard; criterion 13's two words replaced). Task 115
is not git-tracked (RFC-063 §5): `.git-exclude/tasks/dev-team/115-…`; the audit
is Review Request 290 §2. Transcribed from what was run on 2026-09-25; the
mechanics below are RFC-050's code.

---

## §26 acceptance criteria

### 1. orbok checks local model files before downloading.

→ what was run: `verify_ready_current`, `trusted_skip_is_copied_without_a_network_request`
(`crates/pipeline/workers/src/model_delivery.rs`).
→ what was observed: the delivery plan reads the local files first; a valid
current model is never re-fetched.

### 2. Already valid files are skipped.

→ what was run: the same test.
→ what was observed: a trusted, valid file is copied or left, with no network request.

### 3. Missing or invalid files are downloaded.

→ what was observed: the delivery plan downloads what is missing or fails
verification; a file that cannot be verified says so ("could not be verified.
Try again."), through `ModelDeliveryVerification`.

### 4. Partial files are not treated as ready.

→ what was run: `ready_current_rejects_missing_complete_marker_and_corrupt_manifest`.
→ what was observed: a `.part` file, a missing complete marker and a corrupt
manifest are each rejected as not ready.

### 5. Downloads use temporary files and final rename after validation.

→ what was run: `durable_rename_async`, `synced_tokio_file_is_closed_before_durable_rename`,
`promotion_rename_failure_never_registers_or_activates_generation`.
→ what was observed: files are staged and renamed only after validation; a
failed rename never registers or activates a generation.

### 6. The current model download can run two file downloads concurrently.

→ what was observed: the delivery loop starts `plan.max_concurrent` transfers.

### 7. Concurrency is internally limited to 2.

→ what was observed: a plan whose `max_concurrent` is 0 or above 2 is
rejected (`model_delivery.rs`).

### 8. Rate-limit and network failures do not crash the app.

→ what was run: `concurrent_failure_drains_started_transfer_before_cleanup`,
`cancelling_mid_transfer_stops_before_the_next_chunk_is_written`.
→ what was observed: a failed or cancelled transfer drains cleanly, and the
failure pages (`ModelDeliveryConnection` and the others) offer Try again.

### 9. Retry re-checks local files and downloads only what is still needed.

→ what was observed: Try again re-enters the same plan, which skips valid files
(criteria 1–2). It is also what repairs: `ModelRepairingFiles` had no separate
message and is deleted.

### 10. Model is marked ready only after every required file passes validation.

→ what was run: `verify_ready_current`, `promotion_rename_failure_never_registers_or_activates_generation`.
→ what was observed: readiness is decided by the verification, not by the
download finishing.

### 11. Default UI shows one friendly progress experience.

→ what was run: the glossary's formatter scan for `model_file_position` and
`model_transfer_progress` (`crates/ui/src/tests/glossary.rs`).
→ what was observed: the wizard shows "Downloading model…", the file position
and the transfer progress.

### 12. Basic search remains available if better search setup is skipped or fails.

→ what was run: `crates/ui/src/tests/task059_failed_pages_way_out.rs`.
→ what was observed: every failure page has a way out, and **Skip — use
keyword search only** is on the setup pages; keyword search never needs a model.

### 13. No technical download jargon appears in the default UI.

→ what was run: the glossary and the unused-key tests after Task 115 §3; the
consent-screen labels (`ModelConsentRevision`, `ModelArtifactTokenizer`).
→ what was observed: the consent screen reads **Version** (JA 「バージョン」)
where it said "Immutable revision", and, while downloading, **Vocabulary** (JA
「語彙データ」) where it said "Tokenizer"; the value beside "Version" is still the
exact revision identifier. The catalog no longer contains either old word.

---

## Criteria not met, and why RFC-043 closes anyway

None. RFC-043's own copy (§14, §15.1) is superseded by RFC-050's wizard; Amendment
1 (§26a) says which key replaces each.
