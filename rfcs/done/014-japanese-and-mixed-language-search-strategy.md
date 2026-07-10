# RFC-014: Japanese and Mixed-Language Search Strategy

**Project:** orbok  
**RFC:** 014  
**Title:** Japanese and Mixed-Language Search Strategy  
**Status:** Implemented (v0.4.0)
**Target Milestone:** M6/M8 Supporting  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines the Japanese and mixed-language search strategy for `orbok`.

The central decision is:

> `orbok` must not assume whitespace-tokenized English text. It must explicitly support Japanese and mixed Japanese-English documents through normalization, tokenizer strategy, exact identifier preservation, and retrieval quality tests.

This RFC supports both keyword and semantic search.

---

## 2. Motivation

The user base may include Japanese documents and mixed Japanese-English technical materials.

Japanese search differs from English search because:

- words are not separated by spaces;
- full-width and half-width forms vary;
- Latin terms, numbers, and Japanese text are frequently mixed;
- product names and identifiers must be preserved;
- katakana, hiragana, kanji, and romaji may coexist;
- technical documents often contain code symbols.

A naive whitespace-based keyword index will produce poor results.

---

## 3. Goals

- Support Japanese keyword search better than whitespace tokenization.
- Preserve exact identifiers and code-like terms.
- Normalize common width/case variants.
- Support mixed Japanese-English queries.
- Define benchmark/test requirements.
- Keep implementation realistic for local-first Rust app.
- Avoid overclaiming quality before validation.

---

## 4. Non-Goals

- This RFC does not require perfect Japanese linguistic analysis in v1.
- This RFC does not require query translation.
- This RFC does not require cloud NLP.
- This RFC does not require OCR.
- This RFC does not choose final embedding model.
- This RFC does not require semantic search to solve all Japanese synonym issues.

---

## 5. Text Categories

`orbok` should treat documents as mixed-content, not single-language only.

Content classes:

| Class | Examples |
|---|---|
| Japanese natural language | `認証トークンの有効期限` |
| English natural language | `token expiration policy` |
| Mixed Japanese-English | `OAuth クライアント設定` |
| Identifiers | `ABC-1234`, `refresh_token` |
| Paths | `/docs/security/auth.md` |
| Code/log symbols | `ERR_AUTH_TIMEOUT`, `client_secret` |
| Numbers/dates | `2026-06-06`, `v0.19.0` |

---

## 6. Normalization Strategy

Recommended baseline normalization:

- Unicode normalization;
- full-width/half-width normalization for Latin letters and numbers;
- lowercase Latin text for non-exact mode;
- preserve original text for display;
- preserve symbols for exact/code mode;
- normalize common punctuation variants carefully.

Examples:

| Input | Search Normalized |
|---|---|
| `ＡＢＣ－１２３` | `abc-123` or equivalent searchable token |
| `OAuth クライアント` | `oauth クライアント` |
| `refresh_token` | preserve as `refresh_token` |
| `RFC-014` | preserve as `rfc-014` |

Do not destroy symbols that are meaningful in code and identifiers.

---

## 7. Keyword Tokenization Candidates

## 7.1. FTS5 Baseline

SQLite FTS5 `unicode61` is simple but insufficient for Japanese segmentation.

Use only as baseline.

## 7.2. N-Gram Supplemental Index

An n-gram strategy can support Japanese substring matching.

Candidate:

```text
2-gram or 3-gram over normalized Japanese text
```

Pros:

- no heavy morphological dependency;
- robust for unknown terms;
- good for local implementation.

Cons:

- larger index;
- may produce noisy matches;
- ranking requires tuning.

## 7.3. Morphological Tokenizer

A Japanese tokenizer can improve word-level search.

Pros:

- better linguistic tokens;
- better ranking.

Cons:

- dependency size;
- dictionary handling;
- cross-platform packaging;
- mixed technical terms still require care.

## 7.4. Tantivy-Based Strategy

Tantivy may allow richer analyzers than SQLite FTS5.

Pros:

- dedicated search engine;
- analyzer flexibility.

Cons:

- separate index lifecycle;
- more complexity.

---

## 8. Recommended v1 Strategy

Adopt a phased strategy:

## Phase 1: Baseline

- SQLite FTS5 keyword search;
- normalization preserving identifiers;
- tests documenting Japanese limitations.

## Phase 2: Supplemental Japanese Index

Add either:

- n-gram supplemental index; or
- Tantivy backend with Japanese analyzer.

Recommendation:

> Start with n-gram supplemental evaluation before committing to a heavier tokenizer stack.

Reason:

- simpler;
- robust for unknown words;
- suitable for local-first MVP;
- can coexist with FTS5.

## Phase 3: Morphological Tokenizer Evaluation

Evaluate tokenizer-based improvement using benchmark corpus.

---

## 9. Exact Identifier Preservation

Exact identifiers must be searchable regardless of Japanese strategy.

Examples:

```text
ABC-1234
refresh_token
client_secret
ITストラテジスト
v0.19.0
CVE-2026-0001
```

Indexing should create special identifier tokens where practical.

Possible identifier token rules:

- alphanumeric sequences with hyphen/underscore/dot;
- mixed Latin/Japanese technical compounds;
- version-like strings;
- uppercase error codes.

---

## 10. Query Modes

## 10.1. Exact Mode

Exact mode should preserve:

- symbols;
- underscores;
- hyphens;
- dots;
- version numbers;
- case where meaningful.

## 10.2. Auto Mode

Auto mode can normalize more aggressively but must still retain identifier tokens.

## 10.3. Conceptual Mode

Conceptual mode relies more on embeddings, but query normalization still matters.

---

## 11. Semantic Search Considerations

Embedding model choice affects Japanese quality.

Requirements for embedding model evaluation:

- Japanese query to Japanese document;
- Japanese query to English document where concept matches;
- English query to Japanese document where concept matches;
- mixed technical terms;
- exact identifier fallback through keyword search.

Do not rely only on semantic search for identifiers.

---

## 12. Hybrid Search Impact

RRF helps because:

- keyword search can catch exact Japanese/identifier matches;
- vector search can catch conceptual matches;
- fusion avoids overcommitting to one retrieval method.

Search badges should distinguish:

- Exact match;
- Semantic match;
- Hybrid match.

---

## 13. Test Corpus Requirements

Create a small but representative test corpus.

Required document examples:

1. Japanese natural-language Markdown.
2. Mixed Japanese-English technical note.
3. PDF with Japanese text.
4. Source code with Japanese comments.
5. CSV with Japanese headers.
6. Document containing product numbers.
7. Document containing version strings.
8. Document containing security terms in Japanese and English.

---

## 14. Required Query Tests

Examples:

| Query | Expected Behavior |
|---|---|
| `認証トークン` | Finds Japanese auth/token docs |
| `OAuth クライアント` | Finds mixed-language docs |
| `client_secret` | Exact identifier match |
| `ABC-1234` | Exact model number match |
| `トークン 有効期限` | Finds token expiration docs |
| `refresh token expiration` | Semantic or hybrid match |
| `v0.19.0` | Exact version match |
| `システム監査` | Japanese phrase match |

---

## 15. Ranking Metrics

Use:

- top-k recall;
- MRR;
- nDCG;
- exact identifier success rate;
- no-result false negative count;
- query latency;
- index size.

For Japanese search, exact identifier success must be tracked separately from natural-language relevance.

---

## 16. UI Impact

Avoid claiming “perfect Japanese support.”

Suggested copy:

```text
Japanese and mixed-language search is supported, but exact quality depends on the selected search mode and indexing strategy.
```

If current backend is baseline-only:

```text
Japanese exact search is available in baseline mode. Advanced Japanese tokenization is not enabled yet.
```

---

## 17. Storage Impact

N-gram indexing can increase disk usage.

Storage view should attribute this to:

```text
Exact search index
```

If Japanese supplemental index is optional, Settings may expose:

```text
Japanese search enhancement: Off / Balanced / Larger index
```

Do not add this setting until implementation exists.

---

## 18. Acceptance Criteria

- Normalization handles full-width/half-width Latin and numbers.
- Exact identifiers are preserved.
- Japanese baseline tests exist.
- Mixed Japanese-English query tests exist.
- Keyword-only search handles at least simple Japanese substring/term cases.
- Search limitations are documented.
- Hybrid search can combine Japanese keyword and vector results.
- Index size impact is measured before enabling n-gram by default.

---

## 19. Testing Requirements

Required tests:

1. Full-width `ＡＢＣ１２３` matches `ABC123`.
2. `client_secret` remains searchable.
3. `RFC-014` remains searchable.
4. Japanese phrase query finds Japanese document.
5. Mixed `OAuth クライアント` query finds mixed document.
6. English conceptual query finds related Japanese doc if embedding model supports it.
7. Japanese query does not break code symbol search.
8. N-gram index, if enabled, improves recall on Japanese text.
9. Index size increase is measured.
10. Search mode Exact preserves symbols.

---

## 20. Unresolved Questions

- Should n-gram be implemented inside SQLite or separate index tables?
- Should Tantivy be adopted for Japanese search earlier?
- Which Japanese tokenizer is acceptable for licensing and packaging?
- Should hiragana/katakana normalization be attempted?
- Should synonym dictionaries be supported?
- Which embedding model provides acceptable Japanese quality?

---

## 21. Decision

Do not treat Japanese search as an afterthought.

Implement baseline normalization and exact identifier preservation early, then evaluate n-gram or tokenizer-based enhancement with a dedicated test corpus.
