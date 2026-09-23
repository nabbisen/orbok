# Sources and Indexing

## Registering sources

Add folders or files from the **Sources** view. orbok never scans your
whole computer automatically.

When you add a folder near sensitive directories (`.ssh`, `.gnupg`,
`.aws`), orbok shows a warning.

## Hidden files

By default, files and directories starting with `.` are excluded.
Change this per-source via **Edit Policy → Hidden files**.

## Symlinks

The default **Ignore** policy skips symlinks. Use
**Follow within source** to follow links that stay inside the source
root; external links are always rejected.

## Indexing lifecycle

Each file goes through:

1. **Discovered** — found by the scanner
2. **Extracted** — text pulled from the file
3. **Indexed** — chunks in the keyword and vector indexes
4. **Needs update** — file changed since indexing
5. **Missing** — file not found during the last scan

## Force reindex

Pause and resume a source from the Sources view, or use
**Rescan All** from the Indexing view.
