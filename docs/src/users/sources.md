# Folders and Preparing

## Registering sources

Add a folder from the **Folders** view with **Add folder**, or type its path
into the box beside it and press Enter. orbok only searches folders you add;
it never scans your whole computer automatically.

When a folder may contain private files (for example `.ssh`, `.gnupg` or
`.aws`, or your `AppData` folder on Windows or `Library` folder on macOS),
orbok asks first: "Add a folder that may contain private files?".
Nothing is saved or prepared until you choose **Add anyway**; **Cancel** (or
Escape) adds nothing. You can remove a folder at any time with **Remove from
orbok** in **Folders**.

## What a folder covers

A new folder covers **This folder and subfolders**. Each card on the
**Folders** view shows both choices, with a check mark on the one its
folder is set to; pressing the other changes it.

- **This folder only** prepares only the files directly in the folder. To
  choose it, press it on the card. orbok asks first ("Stop including
  subfolders?"): it removes what it prepared for the files in the
  subfolders, and says how many. Your files are never changed or deleted.
  Cancel, or Escape, changes nothing.
- **This folder and subfolders** prepares the subfolders too. Pressing it
  asks nothing and starts preparing.

To prepare only some subfolders, add the folder as **This folder only**,
then add the subfolders you want.

## Folders inside folders

A file belongs to one folder. If you add a folder that is inside a folder
you already added **and covers its subfolders**, orbok adds nothing and says
**Folder already included**: the folder above already covers it. A folder
set to **This folder only** covers nothing below it, so its subfolders can
be added. If you add a folder **above** folders
you already added, they become part of the new one and orbok says **Folders
combined**; what it already prepared for their files is kept, and their
cards go from the list. Changing a folder to **This folder and subfolders**
does the same for added folders inside it. Choosing a folder inside an added
folder to search in registers nothing either: orbok searches the added folder's files, only
those under the folder you chose.

## What orbok skips

orbok does not prepare what is inside a folder in these cases:

- **Files and folders your system hides.** A name that starts with a dot
  (such as `.git`) is hidden everywhere. On Windows, orbok also skips what
  Windows marks **Hidden** or **System**, which includes `AppData`. On macOS
  it skips what macOS marks hidden, which includes `Library`. The folder you
  add is never skipped for being hidden: you chose it.
- **Folders that programming tools create for themselves.** A folder is
  skipped when any of these is true:
  - it holds a `CACHEDIR.TAG` file that starts with the standard signature
    line (Cargo and other tools write one);
  - it is named `node_modules` or `__pycache__`;
  - it is named `target` next to a `Cargo.toml` or `pom.xml` file, or
    `dist` or `build` next to a `package.json` file.

  A folder of yours that happens to be called `target`, `build` or `dist`,
  with none of those files beside it, is prepared like any other.
- **Symbolic links** are not followed.

If a folder you added before now falls under one of these, orbok removes
what it had prepared from it the next time it checks the folder. Your files
are never changed or deleted.

This is fixed; there is no setting for it. To leave part of a folder out,
add the folder as **This folder only** and then add the subfolders you want
(see **What a folder covers**, above).

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
- **Failed** — orbok could not read the file. **Prepare again** on the folder
  tries it again, so fixing the cause (a permission, a reconnected drive) is
  enough.
- **No text** — the file has no text orbok can search.

A file orbok can no longer find shows **File not found** in search
results.

## Preparing again

orbok checks every folder each time it starts, so most changes are picked
up on their own. To ask sooner:

- **A folder:** press **Prepare again** on its card, or `Ctrl/Cmd+R` with
  the folder selected on the **Folders** view. orbok checks the folder for
  changes, prepares what changed, and tries again the files that failed. When
  the card says **Folder not found** or **Cannot open**, it offers **Check
  again** instead.
- **One result:** a result that shows **Needs update** offers **Prepare
  again**.
- **All search data:** turn on **Advanced view** in **Settings**, then open
  **Storage** and choose **Prepare keyword search again** or **Prepare
  search by meaning again**. orbok asks first, removes what it prepared
  and prepares it again. Your files are never changed or deleted, and
  search may be incomplete until it finishes.
