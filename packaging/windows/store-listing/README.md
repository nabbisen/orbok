# Store listing content

Tracked, reviewable source for the Microsoft Store listing. **Publishing
this to Partner Center stays a manual step** — the "Publish to the
Microsoft Store" guide in `docs/src/maintainers/release_readiness.md`
covers it; nothing here is read automatically. This exists so that manual
step has something version-controlled to copy from, instead of text that
only ever existed inside Partner Center.

- `en-us/listing.md`, `ja-jp/listing.md` — every listing field: short and
  long description, product features, what's new, search terms, and the
  screenshot captions, one file per market.
- `en-us/screenshots/`, `ja-jp/screenshots/` — committed PNGs, not a
  promise to regenerate them. Captured by hand against a real, running
  orbok on a scratch profile with fictitious documents; see each
  `listing.md`'s own note on how they were made. Metadata-stripped, pixel
  data only, no path or account name visible in any of them.
- `logo-300.png` — the Store listing's optional 300×300 logo, generated
  from `packaging/icon/orbok-icon.png` by `scripts/generate-icons.sh`.
  Shared across markets (one visual asset, not listing text), so it sits
  here rather than under either `en-us/` or `ja-jp/`.

Two markets exist today, matching `AppxManifest.xml`'s two `<Resource>`
entries (`en-US`, `ja-JP`). A further market gets its own `<lang>/`
sibling directory holding `listing.md` and `screenshots/`. Every market's
`listing.md` must make the same claims about the product — a difference
in wording is expected, a difference in what the app can do is not.

## Manifest versus listing: which wins

`AppxManifest.xml`'s `Description` and `DisplayName` are **OS-facing**
strings — what Windows shows in the Start menu, Task Manager, and package
properties. They come from the manifest and nowhere else; Partner Center
never sees them.

`listing.md`'s short and long descriptions are **shopper-facing**
marketing copy — what appears on the Store product page. They come from
Partner Center (seeded from this file, by hand) and the manifest never
sees them.

These are different surfaces describing the same product, and can drift
into disagreeing about what it does. **`listing.md` is the source.** When
wording changes, it changes here first; the manifest's `Description`
attribute is a short, independent restatement of the same promise, not a
copy of listing text (the manifest schema takes literal text, not a
pointer into this file). Nothing enforces the two staying consistent
mechanically — that is a review responsibility, the same way
`packaging/linux/PKGBUILD`'s `pkgdesc` has no automated tie to this
project's own `README.md` either.

## Privacy policy

Lives at the repository root, `PRIVACY.md` — not under this directory,
since Partner Center takes one privacy policy URL for the whole product,
not one per market. See that file, and the "Publish to the Microsoft
Store" guide for where its URL goes in a submission.
