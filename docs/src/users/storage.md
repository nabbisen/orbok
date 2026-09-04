# Storage and Cleanup

## What orbok stores

orbok does **not** copy your source files. It stores:

| Category | Contents | Deletable? |
|---|---|---|
| Persistent catalog | Source registrations, settings, file metadata | Only via explicit reset |
| Keyword index | FTS5 token index | Yes — rebuilt automatically |
| Vector index | Embedding vectors | Yes — rebuilt from source files |
| Snippet cache | Recent result snippets | Yes — ephemeral |
| Search cache | Cached query results | Yes — ephemeral |
| Temporary extraction | Intermediate extraction output | Yes — rebuilt |

## Safe cleanup

The **Storage** view shows usage per category. Safe cleanup removes:
- Expired snippet and search caches
- Superseded stale index entries

Safe cleanup **never** deletes your source files or source registrations.

## Reset catalog

**Reset catalog** removes all source registrations and indexes. Your
actual files on disk are never touched. This action requires confirmation.

## Storage modes

| Mode | Index size | Features |
|---|---|---|
| Balanced | Moderate | Keyword + semantic |
| High Accuracy | Larger | Richer chunking |
| Space Saving | Smallest | Quantized vectors (future) |
