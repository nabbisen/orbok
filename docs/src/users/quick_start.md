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

orbok stores its catalog and cache in the platform app-data directory:

| Platform | Default path |
|---|---|
| Linux | `~/.local/share/orbok/` |
| macOS | `~/Library/Application Support/orbok/` |
| Windows | `%LOCALAPPDATA%\orbok\` |

Override with `ORBOK_DATA_DIR=/path/to/dir`.

## First launch walkthrough

1. **Add a source folder** — orbok only scans explicitly added folders.
2. **Set up search by meaning (optional)** — the wizard offers to download a local AI model (~490 MB) or lets you skip and use keyword search only.
3. **Wait for indexing** — the Preparing view shows progress.
4. **Search** — type an exact term or a natural-language question.
