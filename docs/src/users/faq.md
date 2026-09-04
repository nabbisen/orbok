# FAQ

**Does orbok upload my files?**
No. All processing is local. Even with an embedding model installed,
inference runs on your computer only.

**Can I search encrypted files?**
Not in v0.x. Encrypted files are skipped with an `unsupported_format` error.

**Why is semantic search unavailable?**
No embedding model is registered. Use the Models view to locate or
install one. Keyword search always works without a model.

**How do I free up disk space?**
Open the Storage view and run Safe Cleanup. There is no separate control for
deleting the vector index; Reset catalog clears the whole derived index and
re-indexes from your source files.

**My source is showing as Stale. What does that mean?**
The source file changed after it was indexed. orbok re-scans every
registered folder each time it starts, and you can refresh a folder on
demand from the Sources view (`[Check again]`/`[Prepare again]`, or
`Ctrl/Cmd+R` with a folder selected) without waiting for a restart.

**How do I search Japanese text?**
Just type normally. orbok detects CJK characters and uses the trigram index
automatically. Full-width ASCII letters and digits are normalized to half-width;
half-width katakana is not.

**Can I use orbok on a server without a display?**
Yes. Run `orbok --check` to validate the backend. Use the orbok-workers
library crate to drive indexing and search programmatically.
