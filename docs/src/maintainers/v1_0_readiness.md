# v1.0.0 Readiness Ledger

This ledger tracks the remaining evidence required before a v1.0.0 release
decision. It is intentionally narrow: it does not open new product scope, add
new RFC requirements, or promote advisory checks.

Controlling scope:

- RFC-019: release readiness gates and manual QA.
- RFC-016: benchmark and retrieval evidence.
- ROADMAP: v1.0.0 requires explicit project-owner confirmation.

Status as of 2026-07-13: post-v0.24.0, pre-v1.0.0.

## Current State

| Area | State | Evidence source |
|---|---|---|
| RFC implementation set | Complete through RFC-046, with RFC-026 archived | [`rfcs/README.md`](../../../rfcs/README.md) |
| Repository-verifiable release gates | Covered by maintainer docs and CI automation | [`release_readiness.md`](release_readiness.md) |
| Keyword-only benchmark evidence | Green for the documented 1,000-document release corpus | [`benchmark_report.md`](benchmark_report.md) |
| `tract` feature build | Blocking feature-matrix gate | [`release_readiness.md`](release_readiness.md) |
| `cargo deny` | Advisory, not release-blocking | [`dep_audit.md`](dep_audit.md) |

## Remaining v1.0.0 Evidence

| Item | Required evidence | Current state |
|---|---|---|
| Real-model benchmark | `orbok-bench-results.json` with `"mode": "hybrid-real-model"` and non-null `model`; recall@5 >= 0.75; p99 <= 200 ms; indexing throughput >= 10 files/s | Failing p99/indexing evidence recorded; RFC-048 recovery in progress |
| Manual QA: Linux | Completed checklist from [`release_readiness.md`](release_readiness.md), including accessibility items | Pending owner sign-off |
| Manual QA: macOS | Completed checklist from [`release_readiness.md`](release_readiness.md), including accessibility items | Pending owner sign-off |
| Manual QA: Windows | Completed checklist from [`release_readiness.md`](release_readiness.md), including accessibility items | Pending owner sign-off |
| Release publication evidence | Final archive, checksum, changelog, tag/release-note confirmation | Pending after RC evidence |
| Owner release decision | Explicit project-owner confirmation to cut v1.0.0 | Pending |

## Real-Model Benchmark Command

Use the guarded command so a keyword-only run cannot accidentally satisfy the
real-model evidence slot:

```sh
cargo run -p orbok-bench --release --features orbok-embed/tract -- \
  1000 target/orbok-bench/results-real-model \
  --model-dir /path/to/multilingual-e5-small \
  --expect-mode hybrid-real-model
```

Archive both generated files for release review:

- `orbok-bench-results.json`
- `orbok-bench-report.md`

## RFC-048 Timing Evidence Checklist

Use this checklist when rerunning the real-model benchmark for RFC-048 Phase 2
bottleneck classification.

Pre-run:

- Confirm the model directory contains `onnx/model.onnx` and `tokenizer.json`.
- Use a fresh output directory, for example
  `target/orbok-bench/results-real-model-rfc048`.
- Record the commit hash or release tag used for the run.
- Record host OS/hardware and runner name.
- Record the model artifact name; do not record a private full filesystem path
  if it is sensitive.

Run:

```sh
cargo run -p orbok-bench --release --features orbok-embed/tract -- \
  1000 target/orbok-bench/results-real-model-rfc048 \
  --model-dir /path/to/multilingual-e5-small \
  --expect-mode hybrid-real-model
```

Required generated files:

- `target/orbok-bench/results-real-model-rfc048/orbok-bench-results.json`
- `target/orbok-bench/results-real-model-rfc048/orbok-bench-report.md`

Gate checks:

- `mode` is `hybrid-real-model`.
- `model` is non-null and records `model_id`, `name`, `version`, and
  `dimension`.
- `recall_at_k.recall >= 0.75`.
- `search_latency_ms.p99_ms <= 200`.
- `indexing_files_per_sec >= 10`.

Timing fields to inspect:

- `timing_ms.corpus_generation_ms`
- `timing_ms.extraction_chunking_keyword_index_ms`
- `timing_ms.model_load_ms`
- `timing_ms.document_embedding_ms`
- `timing_ms.search.query_embedding_ms.p99_ms`
- `timing_ms.search.vector_scan_ms.p99_ms`
- `timing_ms.search.enrichment_ms.p99_ms`
- `timing_ms.search.total_ms.p99_ms`

Evidence to bring back for RFC-048 review:

- command used;
- terminal summary line;
- generated artifact paths;
- key JSON values for mode, model identity, recall, p99 search latency,
  indexing throughput, and the full `timing_ms` object.

## Real-Model Benchmark Evidence Template

Record one entry for the accepted real-model run. This template is for release
evidence capture; it does not change the thresholds in
[`release_readiness.md`](release_readiness.md).

```text
Run date:
Runner:
Host OS / hardware:
orbok version / commit:
Command:
Output directory:

Generated artifacts:
- orbok-bench-results.json:
- orbok-bench-report.md:

JSON evidence:
- mode: hybrid-real-model
- model.model_id:
- model.name:
- model.version:
- model.dimension:

Gate results:
- recall@5:
- p99 search latency:
- indexing throughput:

Result:
- [ ] Pass
- [ ] Fail

Blocking issues:
- None / list issue references

Notes:
-
```

## Manual QA Evidence Template

Record one entry per platform. This template is for owner evidence capture; it
does not replace the checklist in [`release_readiness.md`](release_readiness.md)
or add new release gates.

```text
Platform:
OS version:
orbok version / commit:
Build or archive tested:
Date:
Tester:

Checklist source:
- docs/src/maintainers/release_readiness.md
- docs/src/maintainers/accessibility.md

Result:
- [ ] Pass
- [ ] Pass with notes
- [ ] Fail

Evidence summary:
- First launch:
- Search:
- Storage:
- Models:
- Settings:
- Privacy:
- Accessibility:

Blocking issues:
- None / list issue references

Notes:
-

Owner sign-off:
- [ ] This platform is accepted for v1.0.0 release readiness.
```

## Stop Rule

Do not start new product/design implementation from this ledger. If new work is
needed, open or select an RFC first. If a finding only affects evidence clarity,
make it a separate review unit.
