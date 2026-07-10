# RFC-013: Search View and Result Explanation UX

**Project:** orbok  
**RFC:** 013  
**Title:** Search View and Result Explanation UX  
**Status:** Implemented (v0.4.0)
**Target Milestone:** M9  
**Date:** 2026-06-06  

---

## 1. Summary

This RFC defines the Search View and result explanation UX for `orbok`.

The central decision is:

> Search should be the primary app experience. Results must be understandable, source-connected, and honest about why they appeared and whether the source is current.

The UI should not expose internal terminology by default, but it should provide inspectable ranking details for advanced users.

---

## 2. Motivation

`orbok` combines keyword search, vector search, RRF, and optional reranking. Without careful UX, users may not understand:

- why a result appeared;
- whether it matched exact words or semantic meaning;
- whether source content changed;
- why semantic search is unavailable;
- whether reranking was used;
- why PDF highlights are approximate.

Good search UX must balance simplicity and transparency.

---

## 3. Goals

- Provide a search-first interface.
- Display results with snippets and source paths.
- Explain exact/semantic/hybrid/refined matches.
- Show stale/missing/permission-denied source status.
- Support filters and search modes.
- Provide result preview and advanced details.
- Avoid exposing technical jargon by default.
- Support keyboard navigation and accessibility.

---

## 4. Non-Goals

- This RFC does not define final visual branding.
- This RFC does not implement chat/RAG.
- This RFC does not implement document editing.
- This RFC does not implement multi-user sharing.
- This RFC does not define every responsive breakpoint in final CSS.

---

## 5. Search View Layout

Recommended desktop layout:

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Search                                                                       │
│ ┌────────────────────────────────────────────────────────────┐ [Search]      │
│ │ Ask or search local documents...                            │               │
│ └────────────────────────────────────────────────────────────┘               │
│ Mode: [Auto ▾] Sources: [All ▾] Type: [Any ▾] Refine: [Default ▾]            │
├─────────────────────────────────────┬────────────────────────────────────────┤
│ Results                             │ Preview                                │
│ 24 results · 180 ms · Local only     │                                        │
│                                     │ selected result context                 │
│ result cards                         │ explanation and source actions          │
└─────────────────────────────────────┴────────────────────────────────────────┘
```

---

## 6. Search Modes

User-facing labels:

| Internal Mode | UI Label | Meaning |
|---|---|---|
| auto | Auto | Balanced exact + semantic search |
| exact | Exact | Best for names, codes, identifiers |
| conceptual | Conceptual | Best for meaning-based search |
| fast | Fast | Lower latency, less refinement |
| deep | Deep | More candidates and local refinement |

If semantic search is unavailable, Conceptual and Deep should show warnings or degrade gracefully.

---

## 7. Result Card

```text
┌──────────────────────────────────────────────────────────────┐
│ Token lifecycle policy                                       │
│ /docs/security/auth.md                                       │
│                                                              │
│ ... refresh tokens should expire earlier than long-lived ... │
│                                                              │
│ [Exact match] [Semantic match] [Markdown] [Current]          │
│                                                              │
│ Open file · Open folder · Details                            │
└──────────────────────────────────────────────────────────────┘
```

Required fields:

- title;
- source path;
- snippet;
- source type;
- match badges;
- source status;
- actions.

---

## 8. Result Badges

| Badge | Meaning |
|---|---|
| Exact match | Found by keyword search |
| Semantic match | Found by meaning |
| Hybrid match | Found by both exact and semantic search |
| Refined | Local reranker was used |
| Current | Source file appears current |
| Stale | Source file changed after indexing |
| Missing source | Source file unavailable |
| Temporary | Result came from temporary source |

Badges must contain text, not only color.

---

## 9. Preview Panel

```text
┌──────────────────────────────────────────────────────────────┐
│ auth.md                                                      │
│ /docs/security/auth.md                                       │
├──────────────────────────────────────────────────────────────┤
│ Context                                                      │
│ Section: Security > Token Lifecycle                          │
│ Lines: 120–145                                               │
│                                                              │
│ ... source snippet ...                                       │
├──────────────────────────────────────────────────────────────┤
│ Why this result appeared                                     │
│ Exact match: token expiry                                    │
│ Semantic match: authentication lifecycle                     │
│ Refined: not used                                            │
├──────────────────────────────────────────────────────────────┤
│ [Open File] [Open Folder] [Copy Path] [Inspect Details]      │
└──────────────────────────────────────────────────────────────┘
```

---

## 10. Result Explanation

## 10.1. Default Explanation

Use plain language:

```text
Exact match: token expiry
Semantic match: authentication lifecycle
Refined locally: yes
```

Avoid default labels:

```text
BM25 rank
RRF score
Cross-Encoder score
```

## 10.2. Advanced Details

Advanced drawer may show:

```text
keyword rank
keyword score
vector rank
vector similarity
RRF score
rerank score
embedding model
keyword engine
chunk ID
source hash state
location quality
```

---

## 11. Source Status UX

## 11.1. Stale Result

```text
This file changed after it was indexed. Result may be stale.
[Reindex File] [Open File] [Details]
```

## 11.2. Missing Source

```text
Source file is currently unavailable. The drive may be disconnected or the file may have moved.
[Locate Source] [Remove from Index] [Details]
```

## 11.3. Permission Denied

```text
orbok cannot read this file now. Check file permissions or remove it from the source.
[Retry] [Open Folder] [Details]
```

---

## 12. Degradation Notices

When embedding model is missing:

```text
Semantic search is unavailable. Showing exact search results.
[Open Models]
```

When exact index is rebuilding:

```text
Exact search index is rebuilding. Some exact matches may be missing.
[View Indexing]
```

When reranking times out:

```text
Deep refinement took too long. Showing initial search results.
```

---

## 13. Snippet Rules

Snippets should be loaded dynamically from source file when possible.

If source is unavailable:

- show metadata;
- show missing-source warning;
- do not pretend snippet is fresh.

If location quality is approximate:

```text
Approximate section
PDF page 12
```

Do not show exact line numbers unless location quality supports them.

---

## 14. Filters

Filter drawer:

```text
Sources
File types
Source status
Date modified
Temporary sources
Search mode
```

Default filters:

- include current sources;
- include stale only if setting allows or result has no fresh replacement;
- exclude deleted chunks.

---

## 15. Keyboard and Accessibility

Required shortcuts:

| Shortcut | Action |
|---|---|
| Ctrl/Cmd+K | Focus search |
| Enter | Search |
| Esc | Close drawer/dialog |
| Arrow keys | Move through results when focused |
| Ctrl/Cmd+, | Settings |

Accessibility requirements:

- result cards are keyboard focusable;
- selected result is announced;
- badges are readable by screen readers;
- preview update does not steal focus;
- dialogs trap focus.

---

## 16. Search Empty States

## 16.1. No Sources

```text
Nothing to search yet.
Add a folder or file so orbok can build a local search index.
[Add Source]
```

## 16.2. No Results

```text
No results found.
Try Exact mode for identifiers, Conceptual mode for meaning, or check whether indexing is complete.
```

## 16.3. Model Missing

```text
Semantic search is unavailable.
Keyword search still works.
[Open Models]
```

---

## 17. API Requirements

Search response must include UI-ready metadata:

```json
{
  "results": [
    {
      "title": "Token lifecycle policy",
      "path": "/docs/security/auth.md",
      "snippet": "...",
      "badges": ["exact_match", "semantic_match"],
      "source_status": "current",
      "location_label": "Lines 120-145",
      "explanation": {
        "default": ["Exact match: token expiry"],
        "advanced_available": true
      }
    }
  ],
  "notices": [
    {
      "kind": "semantic_unavailable",
      "message": "Semantic search is unavailable."
    }
  ]
}
```

---

## 18. Privacy Considerations

The Search UI must avoid leaking private content into logs or analytics.

Rules:

- do not log raw queries if history disabled;
- do not log snippets by default;
- do not expose full paths in crash reports without redaction;
- local-only badge should be visible;
- if model download is offered, clarify that documents are not uploaded.

---

## 19. Acceptance Criteria

- User can search from Search View.
- Result cards show title, path, snippet, badges, status.
- Preview panel shows source context.
- Exact/semantic/refined explanation is visible.
- Advanced details are available.
- Stale/missing source states are clear.
- Semantic-unavailable degradation is clear.
- Keyboard navigation works.
- Snippets respect location quality.
- Deleted chunks are not displayed.

---

## 20. Testing Requirements

Required tests:

1. Keyword result displays Exact match badge.
2. Vector result displays Semantic match badge.
3. Hybrid result displays Hybrid or both badges.
4. Reranked result displays Refined badge.
5. Missing source result shows warning.
6. Stale result shows reindex action.
7. Approximate PDF location does not show false line number.
8. Model missing notice appears.
9. Search empty state appears when no results.
10. Keyboard navigation selects results.
11. Query logging respects privacy setting.

---

## 21. Unresolved Questions

- Should result grouping be enabled in v1?
- Should path display be full, shortened, or breadcrumb?
- Should search update live as user types?
- Should reranked results update asynchronously after fused results?
- Should users be able to pin search filters?

---

## 22. Decision

Implement a search-first UI with plain-language result explanation and honest source-status handling.

Advanced ranking details should be inspectable but not prominent by default.
