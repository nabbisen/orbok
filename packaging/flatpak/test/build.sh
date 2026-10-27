#!/usr/bin/env bash
# build.sh — build and install orbok as a local, user-level Flatpak, from
# packaging/flatpak/test/io.github.nabbisen.orbok.yml, for sandbox
# measurement only. Never submitted to Flathub -- the Flathub manifest is
# maintained separately by the owner.
#
# Usage: packaging/flatpak/test/build.sh (from anywhere)
#
# 1. Refuses a dirty working tree, or one not reachable from origin/main --
#    the measurement must always match a real, reviewable commit, not an
#    uncommitted or unpushed change.
# 2. `git archive`s HEAD into a scratch directory -- no network fetch of a
#    release asset, and no binary built here is ever distributed.
# 3. Generates cargo-sources.json from the committed Cargo.lock, fresh,
#    with the upstream flatpak-cargo-generator.py (MIT,
#    flatpak/flatpak-builder-tools), downloaded into the same scratch
#    directory.
# 4. Builds with flatpak-builder --user --install.
#
# Nothing is written outside the scratch directory (removed on exit) and
# the user's own Flatpak installation. Clean up afterward with this
# directory's clean.sh.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$repo_root" || exit 1

APP_ID="io.github.nabbisen.orbok"
MANIFEST="packaging/flatpak/test/io.github.nabbisen.orbok.yml"

[ -f "$MANIFEST" ] || { echo "build.sh: missing $MANIFEST (run from a checkout that has it)" >&2; exit 1; }

# ── Refuse a dirty or unpushed tree: the measurement must match a real,
#    reviewable commit. ────────────────────────────────────────────────
if [ -n "$(git status --porcelain)" ]; then
    echo "build.sh: the working tree is dirty -- commit or stash first" >&2
    git status --short >&2
    exit 1
fi
current_commit="$(git rev-parse HEAD)"
git fetch origin main --quiet
if ! git merge-base --is-ancestor "$current_commit" origin/main; then
    echo "build.sh: HEAD ($current_commit) is not reachable from origin/main -- push first" >&2
    exit 1
fi

# ── Confirm the test app's own data does not already exist -- this
#    script's cleanup (clean.sh) owns that directory, and an existing one
#    might be someone else's. Stop rather than silently reusing or
#    overwriting it. ─────────────────────────────────────────────────────
app_data_dir="$HOME/.var/app/$APP_ID"
if [ -e "$app_data_dir" ]; then
    echo "build.sh: $app_data_dir already exists -- stop and confirm whose data this is before continuing (clean.sh removes it unconditionally)" >&2
    exit 1
fi
if flatpak --user info "$APP_ID" >/dev/null 2>&1; then
    echo "build.sh: $APP_ID is already installed (user) -- uninstall it first (clean.sh does this)" >&2
    exit 1
fi

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
echo "Scratch directory: $scratch"
echo "Building from commit: $current_commit"

# ── The committed tree, archived -- RFC-051's own archive shape (entries
#    rooted at "./", no top-level directory), matching the manifest's own
#    strip-components: 0. ────────────────────────────────────────────────
src_dir="$scratch/src"
mkdir -p "$src_dir"
git archive --format=tar "$current_commit" | tar -x -C "$src_dir"
tar -C "$src_dir" -czf "$scratch/orbok-src.tar.gz" .
rm -rf "$src_dir"

# ── cargo-sources.json, generated fresh from the committed Cargo.lock. An
#    isolated venv (no system pip) for the generator's own PEP 723
#    dependencies; the generator itself is fetched fresh, not vendored. ──
echo "Generating cargo-sources.json from Cargo.lock ..."
venv="$scratch/venv"
python3 -m venv "$venv"
"$venv/bin/pip" install --quiet 'aiohttp<4.0.0,>=3.9.5' 'PyYAML<7.0.0,>=6.0.2' 'tomlkit>=0.13.3,<1.0'
mkdir -p "$scratch/tool"
curl -fsSL -o "$scratch/tool/flatpak-cargo-generator.py" \
    "https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py"
(
    cd "$scratch"
    "$venv/bin/python3" -I "$scratch/tool/flatpak-cargo-generator.py" \
        "$repo_root/Cargo.lock" -o cargo-sources.json
)

# ── The manifest, alongside the archive and cargo-sources.json -- its own
#    `path: orbok-src.tar.gz` and bare `cargo-sources.json` source resolve
#    relative to here, not to the manifest's committed location. ────────
cp "$repo_root/$MANIFEST" "$scratch/io.github.nabbisen.orbok.yml"

# ── Build and install, --user, as an unprivileged local app. ───────────
(
    cd "$scratch"
    flatpak-builder --force-clean --sandbox --user --install \
        --install-deps-from=flathub build-dir io.github.nabbisen.orbok.yml
)

echo
echo "Installed $APP_ID. Permissions:"
flatpak --user info --show-permissions "$APP_ID"
