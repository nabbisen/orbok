#!/usr/bin/env bash
# Task 129 §1.4: prepare a real, hash-verified PKGBUILD for AUR publication.
#
# Writes into $WORK_DIR only -- never pushes, never commits, and never
# touches the AUR itself. Run by hand, from the "Publish to the AUR" guide
# in docs/src/maintainers/release_readiness.md, after a GitHub release
# exists for the requested version. Not wired into CI (the owner's
# instruction): a human runs this when they decide to publish, not on
# every push.
#
# Usage: aur-prepare.sh <version> <work-dir>
#
# Reads packaging/linux/PKGBUILD from the current checkout. Downloads that
# same version's released archive and its .sha256 from GitHub, verifies one
# against the other, computes the real sha256sums line, and writes the
# result to $WORK_DIR/PKGBUILD alongside a freshly generated $WORK_DIR/.SRCINFO.

set -euo pipefail

VERSION="${1:?usage: aur-prepare.sh <version> <work-dir>}"
WORK_DIR="${2:?usage: aur-prepare.sh <version> <work-dir>}"
REPO_PKGBUILD="packaging/linux/PKGBUILD"
ARCHIVE_NAME="orbok-$VERSION.tar.gz"
BASE_URL="https://github.com/nabbisen/orbok/releases/download/$VERSION"

[ -f "$REPO_PKGBUILD" ] || { echo "aur-prepare: missing $REPO_PKGBUILD (run from the repository root)" >&2; exit 1; }

# ── The checked-out PKGBUILD must already claim this version. Automation
#    never writes pkgver -- it comes from a human commit (the same rule
#    forskscope's own aur-publish.sh follows), this script only refuses a
#    mismatch. ─────────────────────────────────────────────────────────────
REPO_PKGVER="$(awk -F= '/^pkgver=/{print $2; exit}' "$REPO_PKGBUILD")"
if [ "$REPO_PKGVER" != "$VERSION" ]; then
    echo "aur-prepare: $REPO_PKGBUILD has pkgver=$REPO_PKGVER, not the requested $VERSION -- checkout the release commit first" >&2
    exit 1
fi

mkdir -p "$WORK_DIR"

# ── Download the release's own archive and its .sha256, and verify one
#    against the other before trusting either (RFC-051: build from the
#    release archive, not GitHub's automatic tag archive). ─────────────────
echo "Downloading $ARCHIVE_NAME and its .sha256 from $BASE_URL ..."
curl -fsSL -o "$WORK_DIR/$ARCHIVE_NAME" "$BASE_URL/$ARCHIVE_NAME"
curl -fsSL -o "$WORK_DIR/$ARCHIVE_NAME.sha256" "$BASE_URL/$ARCHIVE_NAME.sha256"

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
#    below having worked (the same defensive double-check forskscope's own
#    script uses for the same reason: this is the one line most likely to
#    be "obviously fine" and skipped). ──────────────────────────────────────
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
#    metadata, not the template's. ──────────────────────────────────────────
( cd "$WORK_DIR" && makepkg --printsrcinfo >.SRCINFO )

echo "Prepared $WORK_DIR/PKGBUILD and $WORK_DIR/.SRCINFO for pkgver=$VERSION"
