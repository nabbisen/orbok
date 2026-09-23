# Storage and Cleanup

## What orbok stores

orbok does **not** copy your source files. It stores:

| Category | Contents | How to clear it |
|---|---|---|
| Catalog | Source registrations, settings, file metadata | **Reset saved app data...** |
| Keyword index | FTS5 token index | **Prepare keyword search again** |
| Vector index | Embedding vectors | **Prepare search by meaning again** |
| Temporary previews | Recent result snippets | **Clear temporary previews** |
| Old search results | Cached query results | **Clear old search results** |
| Extracted text | Intermediate extraction output | **Clear extracted text** |

The **Storage** view shows up to three lines by default: **Search data**,
**Models** and **Temporary previews**. Turn on **Advanced view** in
**Settings** to see each category above, and the two **Prepare ... again**
buttons. Those two ask first, remove what orbok prepared and prepare it
again from your files.

## Safe cleanup

The **Storage** view shows what orbok stores. Under **Safe cleanup**, four
buttons each remove one kind of data:

- **Clear temporary previews** — recent result snippets
- **Clear old search results** — expired cached query results
- **Clear extracted text** — all extracted-text cache entries (rebuilt
  automatically the next time a file needs it)
- **Remove old data from updated files**

Safe cleanup **never** deletes your source files or source registrations.

## Reset saved app data

**Reset saved app data...** removes all source registrations, indexes, and cached
data (including the extracted-text cache). Your actual files on disk are
never touched. This action requires confirmation.

## Storage modes

| Mode | Index size | Features |
|---|---|---|
| Balanced | Moderate | Keyword search and search by meaning |
| High Accuracy | Larger | Richer chunking |
| Space Saving | Smallest | Quantized vectors (future) |
