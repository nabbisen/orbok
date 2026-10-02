# orbok icon masters

Three tracked masters. Every icon file under `packaging/windows/Assets/`
and `packaging/linux/icons/hicolor/` is generated from one of them by
`scripts/generate-icons.sh`, run by hand and committed — never regenerated
at build time.

| File | Size | Serves |
|---|---|---|
| `orbok-icon.png` | 1024 × 1024 | Every square size from 44 px up: Windows `Square150x150Logo.png`, `Square44x44Logo.png`, `StoreLogo.png`, and Linux hicolor 48/64/128/256/512, plus the window icon and the Store listing's 300×300 logo |
| `orbok-icon-small.png` | 1024 × 1024 | 16 and 32 px only (Linux hicolor), where the full plate's detail stops being legible |
| `orbok-tile-wide.png` | 1240 × 600 | Windows `Wide310x150Logo.png` |

## Where these come from

The owner's source art, `orbok-logo-base-2.png`, has a non-opaque
background (its alpha varies from 0.40 to 0.99 in patches; a widened
variant's from 0.04), which let the surface behind it show through in
blotches on every desktop, taskbar and tile it was placed on. The colour
underneath is a uniform navy, `#02142C`.

The architect corrected this (2026-10-02, at the owner's request) by
building three flattened masters from the same art, changing only the
background, the outline and how the art fits each place — the artwork
itself is unchanged:

- **`orbok-icon.png`** — the art flattened onto opaque `#02142C`, on a
  rounded-square plate (920 px, corner radius 22%) with a transparent
  52 px margin.
- **`orbok-icon-small.png`** — the same plate, with the art cropped to the
  document and magnifier (crop `520×520+200+165` of the original, before
  the plate) so they stay legible once scaled down to 16/32 px.
- **`orbok-tile-wide.png`** — opaque `#02142C`, with the square art scaled
  to 600 px and centred. Built from the square art, not from the widened
  original, whose see-through background showed large X-shaped patches at
  this size too.

**If the owner's art changes**, rebuild these three masters the same way
(flatten onto `#02142C`, same plate geometry, same crops) before
re-running `scripts/generate-icons.sh` — the generator only scales these
masters down; it does not flatten or crop.
