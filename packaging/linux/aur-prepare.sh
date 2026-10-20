#!/usr/bin/env bash
# aur-prepare.sh — prepare a real, hash-verified PKGBUILD for AUR
# publication, for either orbok (source) or orbok-bin (prebuilt binary).
#
# Writes into $WORK_DIR only -- never pushes, never commits, and never
# touches the AUR itself. Run by hand from the "Publish to the AUR" guide
# in docs/src/maintainers/release_readiness.md, or by
# .github/workflows/release-packages.yml's aur-check job, after a release
# exists for the requested version.
#
# Usage: aur-prepare.sh <orbok|orbok-bin> <version> <work-dir>
#
# Reads the package's own PKGBUILD template from the current checkout.
# Downloads that version's release archive and its .sha256 from GitHub --
# or, with LOCAL_ARCHIVE set, copies them from disk instead (the dry run in
# release-packages.yml, where orbok-bin's archive is still job 1's workflow
# artifact, not yet a release asset) -- verifies one against the other,
# computes the real sha256sums line, and writes the result to
# $WORK_DIR/PKGBUILD alongside a freshly generated $WORK_DIR/.SRCINFO.

set -euo pipefail

PKG="${1:?usage: aur-prepare.sh <orbok|orbok-bin> <version> <work-dir>}"
VERSION="${2:?usage: aur-prepare.sh <orbok|orbok-bin> <version> <work-dir>}"
WORK_DIR="${3:?usage: aur-prepare.sh <orbok|orbok-bin> <version> <work-dir>}"

case "$PKG" in
    orbok)
        REPO_PKGBUILD="packaging/linux/PKGBUILD"
        ARCHIVE_NAME="orbok-$VERSION.tar.gz"
        ;;
    orbok-bin)
        REPO_PKGBUILD="packaging/linux/orbok-bin/PKGBUILD"
        ARCHIVE_NAME="orbok-$VERSION-x86_64-linux.tar.gz"
        ;;
    *)
        echo "aur-prepare: unknown package '$PKG' -- expected orbok or orbok-bin" >&2
        exit 1
        ;;
esac
BASE_URL="https://github.com/nabbisen/orbok/releases/download/$VERSION"

[ -f "$REPO_PKGBUILD" ] || { echo "aur-prepare: missing $REPO_PKGBUILD (run from the repository root)" >&2; exit 1; }

# ── The checked-out PKGBUILD must already claim this version. Automation
#    never writes pkgver -- it comes from a human commit, this script only
#    refuses a mismatch. ─────────────────────────────────────────────────────
REPO_PKGVER="$(awk -F= '/^pkgver=/{print $2; exit}' "$REPO_PKGBUILD")"
if [ "$REPO_PKGVER" != "$VERSION" ]; then
    echo "aur-prepare: $REPO_PKGBUILD has pkgver=$REPO_PKGVER, not the requested $VERSION -- checkout the release commit first" >&2
    exit 1
fi

mkdir -p "$WORK_DIR"

# ── Get the release's own archive and its .sha256, and verify one against
#    the other before trusting either (RFC-051: build from the release
#    archive, not GitHub's automatic tag archive). LOCAL_ARCHIVE substitutes
#    a local file pair for the download -- the verification below is the
#    same either way. ─────────────────────────────────────────────────────────
if [ -n "${LOCAL_ARCHIVE:-}" ]; then
    echo "Using local archive $LOCAL_ARCHIVE in place of a release download"
    cp "$LOCAL_ARCHIVE" "$WORK_DIR/$ARCHIVE_NAME"
    cp "$LOCAL_ARCHIVE.sha256" "$WORK_DIR/$ARCHIVE_NAME.sha256"
else
    echo "Downloading $ARCHIVE_NAME and its .sha256 from $BASE_URL ..."
    curl -fsSL -o "$WORK_DIR/$ARCHIVE_NAME" "$BASE_URL/$ARCHIVE_NAME"
    curl -fsSL -o "$WORK_DIR/$ARCHIVE_NAME.sha256" "$BASE_URL/$ARCHIVE_NAME.sha256"
fi

EXPECTED_HASH="$(awk '{print $1; exit}' "$WORK_DIR/$ARCHIVE_NAME.sha256")"
if [ -z "$EXPECTED_HASH" ] || [ "${#EXPECTED_HASH}" -ne 64 ]; then
    echo "aur-prepare: could not read a sha256 from $WORK_DIR/$ARCHIVE_NAME.sha256" >&2
    exit 1
fi
ACTUAL_HASH="$(sha256sum "$WORK_DIR/$ARCHIVE_NAME" | awk '{print $1}')"
if [ "$ACTUAL_HASH" != "$EXPECTED_HASH" ]; then
    echo "aur-prepare: $ARCHIVE_NAME does not match its own .sha256 -- expected $EXPECTED_HASH, got $ACTUAL_HASH. Refusing to publish a package built from an unverified archive." >&2
    exit 1
fi
echo "Verified $ARCHIVE_NAME against its .sha256: $ACTUAL_HASH"

# ── Write the real PKGBUILD. sha256sums=('SKIP') must never reach this
#    point -- checked explicitly rather than trusted to the substitution
#    below having worked. ────────────────────────────────────────────────────
sed "s/^sha256sums=('SKIP')\$/sha256sums=('${ACTUAL_HASH}')/" "$REPO_PKGBUILD" >"$WORK_DIR/PKGBUILD"

if grep -q "sha256sums=('SKIP')" "$WORK_DIR/PKGBUILD"; then
    echo "aur-prepare: sha256sums=('SKIP') would reach the AUR -- refusing to publish" >&2
    exit 1
fi
if ! grep -qF "sha256sums=('${ACTUAL_HASH}')" "$WORK_DIR/PKGBUILD"; then
    echo "aur-prepare: templating did not produce the expected hash line" >&2
    exit 1
fi

# ── .SRCINFO, generated from the real PKGBUILD -- what the AUR itself
#    reads to index the package, so it must be the AUR-ready copy's own
#    metadata, not the template's. ───────────────────────────────────────────
( cd "$WORK_DIR" && makepkg --printsrcinfo >.SRCINFO )

echo "Prepared $WORK_DIR/PKGBUILD and $WORK_DIR/.SRCINFO for $PKG pkgver=$VERSION"
