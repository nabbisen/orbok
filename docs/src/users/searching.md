# Searching

## Search modes

| Mode | Best for |
|---|---|
| **Auto** | General queries (keyword + semantic + RRF) |
| **Exact** | Identifiers, error codes, code symbols |
| **Conceptual** | Natural-language questions |
| **Fast** | Quick lookup, smaller candidate set |

## Exact match tips

- Use the full identifier: `ERR-4042`, `client_secret`, `refresh_token`
- Quote multi-word phrases are not yet supported in v0.x
- Use Exact mode for source code and log searches

## Japanese search

orbok uses both a unicode61 tokenizer and a trigram index. Queries
containing Japanese characters automatically use both. Full-width
characters (Ａ, Ｂ, Ｃ) are normalized to half-width before matching.

## Result badges

| Badge | Meaning |
|---|---|
| Keyword | Matched by the FTS5 keyword index |
| Semantic | Matched by the vector embedding index |
| Needs update | Source file changed since this chunk was indexed |
| Missing source | Source file is currently unavailable |

## Snippets

Snippets are loaded dynamically from the original source file. If the
source file is missing, the snippet shows "(preview unavailable)".
