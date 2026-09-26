# Quick Start

## Requirements

- Rust 1.91+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)

## Install

```sh
cargo install --path crates/app
```

## First use

```sh
# Launch the GUI
orbok

# Headless check (CI / no display)
ORBOK_DATA_DIR=/tmp/orbok-test orbok --check

# Print version
orbok --version

# List options
orbok --help
```

## Data directory

orbok stores its catalog, cache, and model files in the platform app-data
directory, and settings in the platform config directory:

| Platform | Data (catalog/cache/models) | Settings |
|---|---|---|
| Linux | `~/.local/share/orbok/` | `~/.config/orbok/` |
| macOS | `~/Library/Application Support/orbok/` | same directory as data |
| Windows | `%LOCALAPPDATA%\orbok\` | `%APPDATA%\orbok\` (Roaming) |

Default placement is unaffected by anything below and does not change on
any platform.

Override with `ORBOK_DATA_DIR=/path/to/dir` (standard mode only — see
Portable mode below) to relocate the **whole profile — settings included**,
the same relationship Portable mode already has. Before this, the override
relocated only the data directory and left settings at the platform config
path above; it now covers both, so one variable yields one complete,
isolated profile on every platform.

### Portable mode

Run with `--portable` to keep everything — catalog, cache, models, **and
settings** — under `./orbok-data/`, relative to the directory orbok was
started from. Nothing is read from or written to the standard platform
locations above while running this way, and a portable directory that
cannot be created or opened is reported as an error — orbok never falls
back to the standard profile when portable mode fails.

`--portable` and `ORBOK_DATA_DIR` are mutually exclusive: supplying a
non-empty `ORBOK_DATA_DIR` together with `--portable` is rejected outright,
rather than one silently taking precedence. An absent or empty
`ORBOK_DATA_DIR` (including `ORBOK_DATA_DIR=`) counts as not set at all, so
it never triggers that rejection. `ORBOK_DATA_DIR` only ever applies to
standard mode.

The startup message for portable mode shows only the relative
`./orbok-data/` label, not a resolved absolute path — this is a deliberate,
minimal-disclosure default for interactive output. `orbok --check` is an
explicit headless diagnostic command and prints the full resolved path for
either mode, since showing the path is the point of running it.

### Microsoft Store version (Windows)

The Store version keeps its data in the standard place for an app installed
from the Store, which Windows manages for you:

- **Uninstalling orbok also removes its saved data**: your folder list,
  settings, the downloaded model, and everything orbok prepared for search.
  Installing it again starts from the beginning, and preparing large folders
  can take hours. Your own files are never touched.
- **`--portable` is not available**, because the Store installs orbok in a
  folder that cannot be written to. orbok says so and exits without
  creating or opening anything.

## Finding your way around

The sidebar has three items. **Search** is the search page. **AI** holds
**Folders** (where orbok searches), **Preparing** (its progress) and **Models**
(search by meaning). **Settings** holds **Settings** and **Storage**. A group with
several pages shows them as tabs across the top.

## First launch walkthrough

1. **Set up search by meaning (optional)** — until a model is set up, orbok opens **Set up search by meaning**. Choose **Download from HuggingFace** (~490 MB) or enter the folder of a model you already have. **Skip — use keyword search only** leaves it for later; the screen comes back the next time orbok starts.
2. **Add a folder** — under **AI** in the sidebar, on the **Folders** page, choose **Add folder**. orbok only scans folders you add.
3. **Wait for preparing** — the **Preparing** page, next to Folders under **AI**, shows progress.
4. **Search** — type an exact term or a natural-language question.
