# Searching

## Choosing where to search

Type a question and press Search. If no folder is chosen yet, orbok asks you
to choose one; you can also press **Choose a folder** on the search page at
any time. Cancelling keeps what you typed.

Beside the chosen folder, two options, **This folder and subfolders** and
**This folder only**, say where this search looks; the one with a check mark
is chosen, and pressing the other changes it. A folder that is set to **This
folder only** on the **Folders** view shows only its own option: nothing below
it is prepared, so the search looks only in the folder. Choosing a
folder inside a folder you added searches that added folder, limited to the
folder you chose.

## Search modes

The **Mode** choice appears when **Advanced view** is on (**Settings**). By
default orbok uses **Auto**. **By meaning** stays unavailable until search
by meaning is set up.

| Mode | Best for |
|---|---|
| **Auto** | General queries — uses keyword search, and search by meaning when it is set up |
| **Keyword** | Identifiers, error codes, code symbols |
| **By meaning** | Natural-language questions |

## Keyword tips

- Use the full identifier: `ERR-4042`, `client_secret`, `refresh_token`
- Quote multi-word phrases are not yet supported in v0.x
- Use Keyword mode for source code and log searches

## Japanese search

orbok uses both a unicode61 tokenizer and a trigram index. Queries
containing Japanese characters automatically use both. Full-width
characters (Ａ, Ｂ, Ｃ) are normalized to half-width before matching.

## Result badges

Needs update and File not found always show. **Keyword** and **By meaning**
show when **Advanced view** is on.

| Badge | Meaning |
|---|---|
| Keyword | Matched by the FTS5 keyword index |
| By meaning | Matched by search by meaning, using a local model |
| Needs update | Source file changed since this chunk was indexed |
| File not found | orbok cannot find the source file (moved, deleted, or its drive is disconnected) |

## Snippets

Snippets are loaded dynamically from the original source file. If the
source file is missing, the snippet shows "(preview unavailable)".
