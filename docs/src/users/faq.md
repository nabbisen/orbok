# FAQ

**Does orbok upload my files?**
No. All processing is local. Even with an embedding model installed,
inference runs on your computer only.

**Can I search encrypted files?**
Not in v0.x. orbok cannot read the text of an encrypted file, so it is
skipped and never appears in results.

**Why is search by meaning unavailable?**
No embedding model is set up. orbok offers "Set up search by meaning" each
time it starts until one is: choose **Download from HuggingFace**, or
enter the folder of a model you already have (see Local AI Models). The **Models** view shows whether a model is **Available** or
**Missing**. Keyword search always works without a model.

**How do I free up disk space?**
Open the **Storage** view and press a button under **Safe cleanup**:
**Clear temporary previews**, **Clear old search results**, **Clear extracted
text**, or **Remove old data from updated files**. Your folders and search
data stay. **Reset saved app data...** goes further: it removes your
registered folders and all search data, so you would add your folders
again. Your files are never changed or deleted.

**My folder is showing as Needs update. What does that mean?**
A file in the folder changed after orbok prepared it. orbok checks every
registered folder each time it starts, and you can refresh a folder on
demand from the Folders view (**Prepare again**, or `Ctrl/Cmd+R` with a
folder selected) without waiting for a restart.

**How do I search Japanese text?**
Just type normally. orbok detects CJK characters and uses the trigram index
automatically. Full-width ASCII letters and digits are normalized to half-width;
half-width katakana is not.

**Can I use orbok on a server without a display?**
Yes. Run `orbok --check` to validate the backend. Use the orbok-workers
library crate to drive indexing and search programmatically.
