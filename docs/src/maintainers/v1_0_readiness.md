# v1.0.0 Readiness Ledger

This ledger tracks the remaining evidence required before a v1.0.0 release
decision. It is intentionally narrow: it does not open new product scope, add
new RFC requirements, or promote advisory checks.

Controlling scope:

- RFC-019: release readiness gates and manual QA.
- RFC-016: benchmark and retrieval evidence.
- ROADMAP: v1.0.0 requires explicit project-owner confirmation.

Status as of 2026-07-13: post-v0.23.0, pre-v1.0.0.

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
| Real-model benchmark | `orbok-bench-results.json` with `"mode": "hybrid-real-model"` and non-null `model`; recall@5 >= 0.75; p99 <= 200 ms; indexing throughput >= 10 files/s | Pending owner/local model run |
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

## Stop Rule

Do not start new product/design implementation from this ledger. If new work is
needed, open or select an RFC first. If a finding only affects evidence clarity,
make it a separate review unit.
