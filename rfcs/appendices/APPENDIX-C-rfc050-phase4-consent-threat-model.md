# RFC-050 Appendix C — Phase 4 Consent and Threat-Model Delta

**Status:** Accepted bounded Phase 4 consent/threat-model evidence; final consolidation pending
**RFC:** [`../proposed/050-trusted-atomic-model-delivery.md`](../proposed/050-trusted-atomic-model-delivery.md)  
**Handoff:** [`../handoffs/HANDOFF-050-trusted-atomic-model-delivery.md`](../handoffs/HANDOFF-050-trusted-atomic-model-delivery.md)  
**Localization policy:** [RFC-052](../proposed/052-ui-localization-and-design-gate-compliance.md) and [HANDOFF-052](../handoffs/HANDOFF-052-ui-localization-and-design-gate-compliance.md)

This appendix records the accepted bounded Phase 4 consent and threat-model
evidence implemented after Architecture Review 094. It does not by itself
accept Phase 4, move RFC-050, authorize release, or broaden the reviewed
model-delivery worker protocol.

## 1. GUI entry-point and state inventory

The startup wizard is currently the only GUI path that can start installation
of the app-managed default model:

| Entry/state | Responsibility after this slice | Network authority |
|---|---|---|
| `WizardState::NotConfigured` / `FileMissing` | Offers managed download or manual folder selection | None |
| `Message::DownloadModel` | Opens `WizardState::DownloadConsent` with reviewed facts | None |
| `WizardState::DownloadConsent` | Shows provider, source, immutable revision, exact bytes, license, destination, planned verification, and the local-only request boundary | None |
| `Message::ConfirmModelDownload` | Explicit consent event | App adapter only, and only while consent state is active |
| `WizardState::Downloading` | Represents events from the reviewed worker path | None |
| `WizardState::Ready` | Shows `App verified` for managed bytes or `User supplied / provenance not verified` for a manually selected folder | None |
| `AppState::active_model_provenance` / Models view | Retains and displays managed versus user-supplied provenance after the wizard and across ready startup | None |
| `crates/app/src/download.rs` | Adapts the reviewed worker and its typed events | Existing reviewed authority; unchanged by this slice |

`crates/ui` remains a filesystem-, database-, and network-free view-model
boundary. The application supplies the destination path as plain presentation
data. A programmatic confirmation outside `DownloadConsent` is ignored by the
application adapter, so the offer message alone cannot start a request.

The Models page reports installed capability and the separately typed active
model provenance, but has no install action. Startup resolution derives that
provenance from the managed-catalog versus manual-settings boundary and retains
it only when readiness succeeds. Readiness messages represent availability or
repair needs but do not independently start delivery.

## 2. Trust presentation contract

`ModelDownloadConsent::trusted_default` derives model identity, immutable
revision, exact aggregate file size, license, and required app-verification status from
the source-controlled Appendix B manifest. The UI does not copy those trust
facts into a second mutable manifest. Provider is a presentation name for the
manifest's reviewed source service; destination is supplied by the application
because only that layer knows the platform data directory.

The final consent surface also states that documents, searches, source paths,
and the local destination are not sent to the provider. This qualifies the
explicit network action at the decision point; merely saying that the model is
saved locally is not treated as an equivalent privacy statement.

Before download, the status says that orbok will verify the download before
use; it does not claim that absent bytes are already verified. `App verified`
means that app-managed bytes passed the reviewed exact-size and
SHA-256 trust-root checks and the generation protocol. It does not mean that
ONNX or tokenizer formats are intrinsically safe. `User supplied / provenance
not verified` means a manually selected folder passed the lightweight usability
checks; it is deliberately not presented as equivalent provenance.

## 3. Localization boundary and available gates

All copy introduced by this slice uses exhaustive `MessageKey` entries in both
English and Japanese catalogs. Exact byte-size presentation uses a typed
parameterized formatter so the integer byte count is not rounded and its unit
is localized. Dynamic provider, model id, revision, license, and path values are
data, not catalog copy.

Accepted gates include the exhaustive UI catalog tests, parameterized English
and Japanese model-progress formatters, UI/app tests, and bilingual consent,
failure/retry, and persistent-provenance view smoke tests. The progress,
file-position, failure, and retry copy introduced or touched by the bounded
RFC-050 model lifecycle is now typed and localized through that boundary.

RFC-052's repository-wide literal inventory, dedicated policy tooling, CI
wiring, remaining token cleanup, and manual Japanese QA remain pending. This
screen-bounded evidence neither completes RFC-052 nor classifies unrelated
pre-existing UI copy.

## 4. Threat-model delta

### Redirects and request metadata

Consent does not weaken transport policy. Production remains HTTPS-only, uses
the reviewed initial and single redirect host allowlist, rejects relative or
excess redirects, disables environment/system proxy discovery and automatic
referer behavior, and sends no application credentials. Requests necessarily
reveal the reviewed model id, immutable revision, requested file, client network
address, timing, and ordinary HTTP/TLS metadata to the provider/CDN. They do not
need document content, queries, source paths, or the local destination path.
Logs must continue to use logical identifiers and safe error classes rather
than URL query strings or local paths.

### Parser and inference boundary

Digest verification authenticates bytes against reviewed repository metadata;
it does not make ONNX or tokenizer parsing trusted. Both parsers and inference
remain native in-process dependency code, so malformed-parser and dependency
vulnerability risk is residual. Startup validation must continue to fail typed
and leave a generation inactive when tokenizer/ONNX load, a probe inference, or
the expected output dimension fails. No remote validation or document upload is
introduced. Sandboxing remains explicitly out of scope under RFC-050.

### Size and tensor bounds

The trust root fixes exact sizes of 17,082,730 and 470,268,510 bytes and separate
maximum transfer sizes; streaming aborts beyond those bounds. The trusted
identity fixes output dimension 384. The embedding configuration fixes maximum
sequence length and tokenizer truncation/padding before tensor construction,
and later-startup validation checks output dimension. These controls bound the
reviewed normal path but do not prove arbitrary parser allocation safety before
the libraries finish loading authenticated files; that remains residual parser
risk.

### Dependency patch discipline

`reqwest`, `tokenizers`, `tract-onnx`, `tract-core`, and their transitive parsing
and TLS dependencies remain security-relevant. Lockfile review, `cargo audit
--deny warnings`, normal dependency-update review, and rerunning model lifecycle
and load/probe tests are required when these dependencies change. This document
does not claim that the current dependency set is permanently vulnerability
free.

### Residual Windows evidence waiver

Architecture Review 093 closed one unsupported-storage junction case by a
strict evidence waiver, not by test execution. The ignored physical fixture and
all automatic revocation conditions remain active. This consent slice changes
no Windows storage policy, reparse handling, volume identity, durability helper,
or worker call site, and therefore does not consume or broaden that waiver. Any
future relevant change or contradictory result must revoke the affected
evidence disposition and return it to architecture/security review.

## 5. Stop point

The GUI lifecycle integration and Appendix D's named compositional proof are
implemented and independently reviewed. Final Phase 4 consolidation remains
the next review stop. RFC lifecycle movement, release-readiness claims, and the
repository-wide RFC-052 program remain later independent decisions.
