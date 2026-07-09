# orbok

[![crates.io](https://img.shields.io/crates/v/orbok?label=rust)](https://crates.io/crates/orbok)
[![Rust Documentation](https://docs.rs/orbok/badge.svg?version=latest)](https://docs.rs/orbok)
[![Dependency Status](https://deps.rs/crate/orbok/latest/status.svg)](https://deps.rs/crate/orbok)
[![License](https://img.shields.io/github/license/nabbisen/orbok)](LICENSE)

**Local-first AI document search — private, storage-aware, offline.**

---

## Overview

orbok searches your local files by combining exact keyword retrieval
and dense vector (semantic) search, fused with Reciprocal Rank Fusion,
with optional local reranking. Everything runs on your computer.
Document contents are never sent to an external server.

Supported document types (current): plain text, Markdown, HTML, PDF, DOCX, CSV,
and common source-code files.

---

## Why orbok

| Need | orbok |
|---|---|
| Search by exact identifier or error code | Keyword index (FTS5) |
| Search by meaning or concept | Local embedding model |
| Privacy — no cloud upload | All processing is local by default |
| Mixed Japanese and English documents | Unicode tokenization (RFC-014 refines) |
| Understand what is stored | Storage dashboard with cleanup controls |

---

## Quick Start

```sh
# Install (requires Rust 1.85+)
cargo install --path crates/app

# Launch the GUI
orbok

# Validate backend without a display (CI / headless)
ORBOK_DATA_DIR=/tmp/orbok-test orbok --check
```

On first launch, orbok asks you to add at least one source folder.
It will scan that folder and build a local search index.

Semantic search requires a local embedding model; keyword search
works with no models installed at all.

---

## Design Notes

### Local-first by design

orbok does not copy your source files. It stores derived indexes
(chunk offsets, FTS5 tokens, embeddings) and metadata. Full extracted
text is not stored permanently by default.

Data is classified into three lifecycle layers (RFC-001):

- **Persistent catalog** — source registrations, settings, file metadata.
  Never deleted by routine cleanup.
- **Rebuildable indexes** — keyword index, embedding vectors. Deletable
  and rebuildable from source files at any time.
- **Ephemeral cache** — recent snippets, search result cache. LRU-evicted.

### Security boundary

The Rust backend enforces a strict source allowlist (RFC-003):
the frontend requests data through typed service calls; it never
reads arbitrary filesystem paths.

### Keyword search

SQLite FTS5 with unicode61 tokenization. The index is contentless:
tokens are indexed, but source text is not retained. Display snippets
are loaded dynamically from source files via stored byte offsets.

Japanese segmentation is in the roadmap (RFC-014).

### Semantic search (optional)

Local dense embedding via a pluggable Rust inference backend (ONNX
Runtime via tract). Model files stay on your machine. Switching
models marks existing embeddings stale and queues a rebuild.

### Disk use

orbok keeps separate databases: `orbok-catalog.sqlite3` for the
authoritative catalog and `orbok-cache.sqlite3` for the localcache
payload store (per Appendix A of the RFC set).

---

## More Detail

Full documentation is in `docs/` (mdbook):

- [Features and tutorials](docs/src/users/features.md)
- [Architecture overview](docs/src/maintainers/architecture.md)
- [RFC index](rfcs/README.md)
