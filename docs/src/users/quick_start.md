# Quick Start

## Requirements

- Rust 1.85+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)

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
```

## Data directory

orbok stores its catalog, cache, and model files in the platform app-data
directory:

| Platform | Default path |
|---|---|
| Linux | `~/.local/share/orbok/` |
| macOS | `~/Library/Application Support/orbok/` |
| Windows | `%LOCALAPPDATA%\orbok\` |

Override with `ORBOK_DATA_DIR=/path/to/dir` (standard mode only — see
Portable mode below).

### Portable mode

Run with `--portable` to keep everything — catalog, cache, models, **and
settings** — under `./orbok-data/`, relative to the directory orbok was
started from. Nothing is read from or written to the standard platform
locations above while running this way.

`--portable` and `ORBOK_DATA_DIR` are mutually exclusive: supplying a
non-empty `ORBOK_DATA_DIR` together with `--portable` is rejected outright,
rather than one silently taking precedence. `ORBOK_DATA_DIR` only ever
applies to standard mode.

The startup message for portable mode shows only the relative
`./orbok-data/` label, not a resolved absolute path — this is a deliberate,
minimal-disclosure default for interactive output. `orbok --check` is an
explicit headless diagnostic command and prints the full resolved path for
either mode, since showing the path is the point of running it.

## First launch walkthrough

1. **Add a source folder** — orbok only scans explicitly added folders.
2. **Set up search by meaning (optional)** — the wizard offers to download a local AI model (~490 MB) or lets you skip and use keyword search only.
3. **Wait for indexing** — the Preparing view shows progress.
4. **Search** — type an exact term or a natural-language question.
