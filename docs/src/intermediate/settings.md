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
