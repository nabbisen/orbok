# Settings Reference

## Index Quality

| Setting | Values | Default |
|---|---|---|
| `index.quality_mode` | `balanced`, `high_accuracy`, `space_saving` | `balanced` |

## Search

| Setting | Values | Default |
|---|---|---|
| `search.default_mode` | `auto`, `exact`, `conceptual`, `fast` | `auto` |
| `search.rerank_default` | `enabled`, `disabled` | `disabled` |

## Storage

| Setting | Type | Default |
|---|---|---|
| `storage.cache_limit_bytes` | integer | `8589934592` (8 GiB) |

## Privacy

| Setting | Values | Default |
|---|---|---|
| `privacy.search_history_retention` | `none`, `session` | `none` |
| `ui.locale` | `en`, `ja` | `en` |

## Environment variables

| Variable | Purpose |
|---|---|
| `ORBOK_DATA_DIR` | Override the whole local profile — catalog, cache, models, and settings — standard mode only; rejected if combined with `--portable` |
| `RUST_LOG` | Tracing log level (e.g. `orbok=debug`) |

See [Quick Start: Data directory](../users/quick_start.md#data-directory)
for the full standard/portable precedence rule.

## When the platform configuration directory is unavailable

Standard mode without an `ORBOK_DATA_DIR` override needs the platform
configuration directory (`~/.config/orbok/` on Linux, `%APPDATA%\orbok\` on
Windows, and so on) to place `settings.json`. In environments without a
resolvable user environment — containers, service accounts, kiosks — that
directory may not exist, and orbok reports this as a startup error rather
than silently falling back to a substitute location. The error message
names both remedies:

- Run with `--portable`, which never touches the platform configuration
  directory at all (see [Quick Start: Portable
  mode](../users/quick_start.md#portable-mode)).
- Set `ORBOK_DATA_DIR` to a writable directory, which relocates settings
  alongside the rest of the profile in standard mode.

Both modes start normally in this situation — only unoverridden standard
mode requires the platform configuration directory to exist.
