# RFC-064: What a Folder Covers

**Project:** orbok\
**RFC:** 064\
**Title:** What a Folder Covers\
**Status:** Accepted\
**Accepted:** 2026-09-24 by the project owner\
**Target milestone:** folder management\
**Date:** 2026-09-24\
**Related RFCs:** RFC-003 (registration; Amendment 1 §10a keeps settings few); RFC-004 (the scanner, "recursively scan registered sources"); RFC-045 (search in a folder, and its two scopes); RFC-059 (erasure completeness); RFC-041 (plain language)

---

## 1. Summary

A folder the user adds covers either **this folder and its subfolders**
(the default, as today) or **this folder only**. The user sees the choice
on the folder's card and can change it at any time. One rule makes the
choice safe:

> **Every file belongs to at most one added folder.**

Today nothing enforces that rule, and orbok breaks it (§2).

## 2. Motivation

1. **No way to leave subfolders out.** Preparation is always fully
   recursive (RFC-004). A user with a large tree, or with subfolders they do
   not want prepared, can only avoid it by not adding the folder.
2. **Overlapping folders are prepared twice.** `add_source` rejects only an
   exact duplicate path. Adding `~/docs` and then `~/docs/work` registers
   both, prepares every file under `work` twice, and shows it twice in
   results. RFC-045's search-in-folder flow does this without the user
   asking: choosing a subfolder of an added folder to search in registers
   the subfolder as a second folder.
3. **A search choice that changes nothing.** RFC-045 offers "This folder
   only" and "This folder and subfolders" when searching. That is honest
   only while everything below the folder is prepared.

## 3. Decisions

### 3.1. Two choices, not a depth

The choices are **This folder and subfolders** and **This folder only**.
They are the labels RFC-045 already shows (`SearchScopeSubfolders`,
`SearchScopeOnly`), so there is one vocabulary for what a folder covers and
what a search looks in.

**There is no depth setting.** "Two levels deep" is hard to picture and
hard to check. The same result, and a clearer one, comes from adding a
folder as **This folder only** and adding the particular subfolders wanted
beneath it. The one-folder rule (§3.3) allows exactly that.

### 3.2. One place to choose: the folder's card

- **A new folder covers its subfolders**, as today. Adding stays one step:
  no extra question for the common case, and Task 110's question for a
  folder that may hold private files is untouched.
- **The card shows the current choice and changes it.** The control has
  the same shape as the search row's scope toggle (RFC-045 §11.2). It is
  not repeated in Settings or at add time: one setting, in one place.
- **Narrowing** (subfolders → this folder only) **asks first**, in Task
  062's dialog shape, with a counted line. Then:
  - orbok cancels queued work for files that fall out of the folder;
  - it removes what it prepared for them, completely (RFC-059): chunks,
    keyword and meaning index rows, and cache entries;
  - it drops their catalog rows. Those files are **out of the folder**,
    not **missing**, and are never shown as File not found.
- **Widening** (this folder only → subfolders) needs no question. It adds
  nothing the user did not ask for, and it starts preparing.

### 3.3. Every file belongs to at most one folder

| The user… | orbok… |
|---|---|
| adds a folder that an added folder **with subfolders** already covers | adds nothing, and says the folder is already included, naming the folder that includes it |
| adds a folder **above** added folders, or widens a folder so that it covers added folders | makes those folders part of it. Their prepared data is kept where it can be moved safely in one transaction; otherwise it is prepared again. Their cards go, and a notice names them |
| adds a subfolder of a folder set to **this folder only** | adds it. There is no overlap, and this is how part of a tree is chosen |
| searches in a subfolder of an added folder (RFC-045) | searches the added folder's data, limited to that subfolder, **and registers nothing** |

**Existing profiles** may already hold overlapping folders. At startup,
orbok applies the second row once and shows the same notice.

### 3.4. The search follows what the folder covers

A search in a folder set to **this folder only** shows that scope and
offers no toggle, since nothing below is prepared. A search in a folder
with subfolders keeps both choices.

### 3.5. What goes away

`SourcesRecursiveHint` ("All sub-folders are scanned recursively.") is
removed. Each card now says what it covers.

## 4. Copy (owner-approved 2026-09-24)

| Where | EN | JA |
|---|---|---|
| Card, the choice | This folder and subfolders / This folder only (existing) | このフォルダーとサブフォルダー / このフォルダーのみ (existing) |
| Narrowing, title | Stop including subfolders? | サブフォルダーを含めないようにしますか? |
| Narrowing, body | orbok removes what it prepared for files in this folder's subfolders. Your files are never changed or deleted. | orbok はこのフォルダーのサブフォルダーにあるファイルについて、準備したデータを削除します。ファイルは変更も削除もされません。 |
| Narrowing, counted line | This removes what orbok prepared for {files} files. | ファイル {files} 件について、準備したデータを削除します。 |
| Narrowing, confirm | Stop including | 含めない |
| Already included, title | Folder already included | フォルダーはすでに含まれています |
| Already included, body | {folder} is already part of {parent}. | {folder} は {parent} に含まれています。 |
| Folders combined, title | Folders combined | フォルダーをまとめました |
| Folders combined, body | {folders} now part of {parent}. (singular: "{folder} is now part of {parent}.") | {folders} は {parent} に含まれるようになりました。 |

- Cancel reuses `Cancel`.
- The body carries Task 100's canonical promise word for word.
- The counted line follows `fmt_rebuild_prepares`'s singular/plural
  pattern; no count, no line.

## 5. Acceptance criteria

1. A card shows what its folder covers, and changes it.
2. **This folder only** prepares only the files directly in the folder.
3. Narrowing asks first, with a counted line. Afterwards, no chunk, index
   row, cache entry or catalog row remains for a file that fell out of the
   folder, and none is shown as File not found.
4. Widening prepares the subfolders without asking.
5. No file is ever registered under two folders, whichever order the user
   adds, widens or searches in (§3.3, all four rows).
6. Searching in a subfolder of an added folder registers nothing and finds
   only files in that subfolder.
7. A search in a **this folder only** folder offers no scope toggle.
8. Overlapping folders in an existing profile are combined once at
   startup, with the notice.
9. `SourcesRecursiveHint` is gone; the glossary and the unused-key test
   (Task 109) stay green.

## 6. Out of scope

A depth setting (§3.1). Excluding named subfolders: RFC-003 Amendment 1
dropped include/exclude patterns.
