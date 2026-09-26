# Local AI Models

## Why models?

Keyword search works without any models. Search by meaning requires a
local embedding model.

## Privacy guarantee

Model files are stored on your computer. They are used only for local
inference. Documents are **never** sent to the model provider's servers.

## Setting up a model

Until a model is set up, orbok opens **Set up search by meaning** each time
it starts. You can choose **Skip — use keyword search only**; the screen
comes back the next time orbok starts.

- **Download from HuggingFace:** orbok shows what it will download (the
  provider, the exact size and the license) under **Review model
  download**. **Agree and download** starts it, and orbok verifies the
  files before it uses them.
- **Files you already have:** choose **Choose a folder** and pick the folder
  that contains `onnx/model.onnx` and `tokenizer.json`, or type or paste its
  path into **Folder path…** and press Enter. orbok checks the folder as soon
  as you choose it, and says which file is missing if one is. When it has
  found both files, choose **Use this model**.

When orbok has the model, it says **The model is ready to use** and offers
**Use this model**; choose **Back** to pick another folder instead. Search by
meaning turns on when you choose **Use this model**.

The **Models** view only shows the state: **Search by meaning** is
**Available** or **Missing**. It has no buttons.

## Changing the embedding model

If you switch embedding models, what orbok already prepared for search
by meaning becomes incompatible. orbok does not currently invalidate or
rebuild it automatically — vectors indexed under the previous model remain and are
not used by the new one. After a model change, choose **Prepare search by
meaning again** (turn on **Advanced view** in **Settings**, then open
**Storage**).

## Recommended models

See the [orbok model compatibility list](https://github.com/nabbisen/orbok/wiki/Models)
(external link, maintained separately).
