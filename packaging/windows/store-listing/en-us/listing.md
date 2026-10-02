# orbok — Microsoft Store listing (en-US)

Approved by the owner, 2026-10-02. Described version: 0.27.0 — nothing
here promises anything 0.27.0 does not do.

## Properties and fields

| Field | Value |
|---|---|
| Product name | orbok (already reserved) |
| Category | Productivity |
| Pricing | Free |
| Website | `https://github.com/nabbisen/orbok` |
| Support contact info | `https://github.com/nabbisen/orbok/issues` (a URL, not an email) |
| Privacy policy URL | `https://github.com/nabbisen/orbok/blob/main/PRIVACY.md` |
| Copyright and trademark info | © 2026 nabbisen |
| Additional license terms | Licensed under the Apache License, Version 2.0: `https://github.com/nabbisen/orbok/blob/main/LICENSE` |
| "Does this product access, collect or transmit personal information?" | No. Documents, searches and folder paths stay on the computer; the one optional download sends none of them (see `PRIVACY.md`). |
| Languages | English (en-US), Japanese (ja-JP), as declared in the manifest |
| Minimum system | Windows 10 version 2004 (build 19041) or later, x64 (from the manifest) |
| Age rating (IARC questionnaire) | Answer No to every content question: no violence, no user-to-user communication, no sharing of location or personal information, no purchases, no unrestricted web access. The expected result is the lowest rating (3+ / Everyone) — Microsoft's decision, not ours. |

## Short description

> Find what's in your documents by what you mean, not only the exact words.
> Search stays on your computer: nothing is uploaded.

## Description

> orbok searches the documents on your own computer, in the folders you choose.
>
> **Search by meaning.** Ask in your own words, such as "how much will the trip
> cost", and orbok finds the passage about the budget even when it never uses
> those words. It works across many languages, and can find a document
> written in one language from a question asked in another. Keyword search
> works too, for names, codes and exact phrases.
>
> **Private by design.** Your documents are processed on your computer only.
> orbok has no account, no cloud service, no analytics and no advertising.
> Search by meaning uses a local AI model. orbok asks before downloading it
> (about 490 MB, once, from Hugging Face), and sends nothing about your files
> or searches when it does. Keyword search works without it.
>
> **Your files stay yours.** orbok reads only the folders you add, and never
> scans your whole computer. Your files are never changed or deleted. Before
> adding a folder that may contain private files, such as SSH keys or
> application data, orbok asks first. Files your system keeps hidden are
> skipped.
>
> **Clear and calm to use.**
> - Choose whether a folder includes its subfolders.
> - Watch preparation as it happens.
> - Results show which file and section each match is in, and say plainly
>   when a file has changed since orbok prepared it.
> - Light, dark and two high-contrast themes, three text sizes, reduced
>   motion, and keyboard shortcuts.
>
> **Storage you can see.** orbok shows what it stores and lets you clean up
> temporary data safely.
>
> Supported files include text, Markdown, PDF, Word (.docx), HTML, CSV and
> common source-code files. The app itself is in English and Japanese.
>
> orbok is open source under the Apache License 2.0.

## Product features

1. Search by meaning: ask in your own words, and find passages that match what you mean.
2. Keyword search for names, codes and exact phrases.
3. Works across many languages: a question in one language can find a document in another.
4. Documents are processed on your computer only: no account, no cloud, no analytics.
5. Searches only the folders you choose; your files are never changed or deleted.
6. Asks before adding a folder that may contain private files.
7. Light, dark and high-contrast themes, three text sizes, reduced motion and keyboard shortcuts.

**Why "many languages", and why "your computer":**

- **Languages.** The interface language (English, Japanese) and the languages
  search by meaning understands are separate things. The model,
  `multilingual-e5-small`, is trained on about 100 languages (its model
  card, which inherits the XLM-RoBERTa language set); its quality is lower
  for languages with less training data. Tested here: English and
  Japanese, both directions. The listing therefore says "many languages",
  gives no number, and lists no languages it has not shown. Keyword search
  is narrower: it handles languages written with spaces between words,
  plus Japanese, Chinese and Korean (through its trigram index) — the
  listing makes no language claim for it.
- **"Your computer".** Inside the app, "this computer" is right: it is the
  machine the user is sitting at, and it is the app's one approved wording
  for the promise. In a Store listing, read before installing, "this
  computer" has no clear referent. "Your computer" says the same promise
  to that reader. The app's own wording is unchanged; screenshot 3 shows
  it as the app says it, and its caption (listing text) says "your
  computer".

## What's new in this version (0.27.0)

> - Fixed: a file could drop out of search during a check of its folder.
> - Fixed: upgrading could reset your settings.
> - A folder can include or leave out its subfolders.
> - orbok asks before adding a folder that may contain private files.
> - Files your system keeps hidden are skipped.
> - Every setting shows what it is set to.

## Additional system requirements

> Search by meaning needs a one-time download of about 490 MB and about 500 MB
> of free disk space. Keyword search works without it.

## Search terms (up to 7)

`document search` · `local search` · `semantic search` · `private search` ·
`offline search` · `PDF search` · `AI search`

Search terms are hidden matching keywords, not text a user reads. They use
the words people type into the Store, such as "semantic". The app's own
wording stays "search by meaning".

## Screenshots

Format: PNG, 1600 × 900 (Microsoft's desktop minimum is 1366 × 768). Files
in `screenshots/`, uploaded in this order:

| # | File | Caption |
|---|---|---|
| 1 | `en-1-search-by-meaning.png` | Ask in your own words. orbok finds the budget section, and a Japanese document too. |
| 2 | `en-2-search-dark-theme.png` | "How do I look after my bike" finds the bicycle checklist, in the dark theme. |
| 3 | `en-3-settings-privacy.png` | Your documents are processed on your computer only. Themes, text size and reduced motion. |
| 4 | `en-4-first-run.png` | Search by meaning is optional. Nothing is uploaded, and keyword search works without it. |

**How they were made** (so they can be repeated): release builds of
`main` on a scratch profile, the real `multilingual-e5-small` model, the
window sized to 1600 × 900, the search queries shown in the images
themselves. Screenshots 1–2 were retaken 2026-10-02 (commit `0cb6b11`,
Task 128's cleaner result cards); 3–4 are unchanged since commit `a196d28`.

**One honest limit:** they were taken on Linux. orbok draws its own
interface, so a Windows screen shows the same layout and words — font
rendering and the system folder picker are where a Windows screenshot
could differ. If Windows-native captures are wanted, retake the same four
screens on Windows under a neutral account name (for example `demo`),
with these same sample documents, and still never the Folders page (its
cards show the folder's full path, which contains the computer account's
name).
