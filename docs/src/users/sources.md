# Folders and Preparing

## Registering sources

Add a folder from the **Folders** view with **Add folder**, or type its path
into the box beside it and press Enter. orbok only searches folders you add;
it never scans your whole computer automatically.

When you add a folder near sensitive directories (`.ssh`, `.gnupg`,
`.aws`), orbok adds it and shows the notice "This folder may contain
private files". Remove the folder if you did not mean to search it.

## What orbok skips

Hidden files and folders (names starting with `.`) are not prepared, and
symbolic links are not followed. This is fixed; there is no setting for it.

## What you see while a folder is prepared

A folder's card on the **Folders** view shows its state: **Preparing**
while orbok still has work to do on it, then **Ready** or **Needs update**;
or **Folder not found** or **Cannot open** when orbok cannot reach it. The
card updates as orbok works, so you can watch the counts rise. Under the
state, a line counts the folder's files that are **Ready**, **Needs update**,
**Failed** or have **No text**. The **Preparing** view adds up **Ready**,
**Needs update** and **Failed** for every folder and shows how many files
are **Queued**.

- **Ready** — the file is prepared for search.
- **Needs update** — the file changed after orbok prepared it.
- **Failed** — orbok could not read the file.
- **No text** — the file has no text orbok can search.

A file orbok can no longer find shows **File not found** in search
results.

## Preparing again

orbok checks every folder each time it starts, so most changes are picked
up on their own. To ask sooner:

- **A folder:** press **Prepare again** on its card, or `Ctrl/Cmd+R` with
  the folder selected on the **Folders** view. orbok checks the folder for
  changes and prepares what changed. When the card says **Folder not
  found** or **Cannot open**, it offers **Check again** instead.
- **One result:** a result that shows **Needs update** offers **Prepare
  again**.
- **All search data:** turn on **Advanced view** in **Settings**, then open
  **Storage** and choose **Prepare keyword search again** or **Prepare
  search by meaning again**. orbok asks first, removes what it prepared
  and prepares it again. Your files are never changed or deleted, and
  search may be incomplete until it finishes.
