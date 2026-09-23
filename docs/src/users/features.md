# Features

orbok provides local-first AI document search combining:

- **Keyword search** (FTS5): finds identifiers, error codes,
  product numbers, and code symbols precisely.
- **Search by meaning** (optional): finds content related in meaning
  using a local embedding model — no cloud upload.
- **Hybrid ranking** (RRF): blends the results of keyword search and
  search by meaning.
- **Japanese and mixed-language support**: trigram index for CJK text,
  full-width→half-width normalization.

All processing is local. Documents never leave your machine.
