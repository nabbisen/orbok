# RFC-065: orbok on Flathub

**Project:** orbok\
**RFC:** 065\
**Title:** orbok on Flathub\
**Status:** Accepted\
**Accepted:** 2026-10-21 by the project owner, with the application ID (§3.1) and the summary line (§3.4)\
**Target milestone:** Linux distribution (the release after 0.29.0)\
**Date:** 2026-10-21\
**Related RFCs:** RFC-017 (packaging and distribution); RFC-051 (the reviewed source archive); RFC-064 (every file belongs to one folder); RFC-041 (plain language)\
**Related tasks:** Task 129 (the AUR recipe, the desktop entry); Task 132 (the folder picker and its portal probe); Task 133 (release packages behind the owner's approval)

---

## 1. Summary

Publish orbok on **Flathub** as `io.github.nabbisen.orbok`, so that users
of any Linux distribution can install it from their software center.

- **Flathub builds it on its own servers,** from the reviewed source
  archive (RFC-051) and a vendored `Cargo.lock`. No binary built on a
  developer machine is distributed.
- **The sandbox is narrow:** read-only access to the home folder, network
  only for the optional model download, and nothing else beyond what a
  windowed app needs.
- **Releases reach Flathub the way they reach the AUR.** The release
  workflow prepares the update and pauses for the owner's approval.
  Flathub's own pull request is then the second, visible check.

## 2. Motivation

- **Reach.** The AUR serves Arch users only. Flathub is available on
  nearly every desktop Linux distribution, and is the default software
  source on several of them.
- **The sandbox matches what orbok promises.** "Your files are never
  changed or deleted" becomes something the system enforces (read-only
  access), not only something orbok says.
- **One more channel must not mean one more process.** It reuses the
  archive, the checksum and the approval gate that already exist.

## 3. Decisions

### 3.1 The application ID: `io.github.nabbisen.orbok`

- **Approved by the owner, 2026-10-21.** It is permanent on Flathub.
- It is verified through the owner's GitHub account. No domain and no
  email address are involved.
- **One ID on every Linux package.** Flathub requires the desktop entry,
  the icons and the metadata file to be named by the ID, and the window's
  Wayland `app_id` to match the desktop entry. So:
  - `packaging/linux/orbok.desktop` becomes `io.github.nabbisen.orbok.desktop`;
  - the hicolor icons become `io.github.nabbisen.orbok.png`;
  - the window's `application_id` (`"orbok"` today, Task 129) becomes the
    ID, and `StartupWMClass` follows it.

  The AUR packages install the same renamed files. Two ways of naming
  orbok's window on Linux would be the kind of split standard the project
  avoids.
- **The command stays `orbok`.** Only desktop-facing names change.

### 3.2 The sandbox

| Permission | Why |
|---|---|
| `--filesystem=home:ro` | Folders are added by path and prepared again later, so orbok needs lasting read access to them. **Read-only**, because orbok never writes to the user's files. |
| `--share=network` | Only the optional model download, which asks first (`PRIVACY.md`). |
| `--socket=wayland`, `--socket=fallback-x11`, `--share=ipc`, `--device=dri` | A GPU-drawn window (iced/wgpu). |
| *(none)* | No `host` filesystem access, no session or system bus beyond the portals Flatpak always allows, no devices beyond the GPU. |

- **Outside the home folder** (external drives, `/mnt`), a folder is
  reached through the system folder picker, which grants access to that
  folder alone (§5.1).
- **Where orbok writes:** its own data lives in
  `~/.var/app/io.github.nabbisen.orbok/`, because `dirs::data_local_dir()`
  follows the sandbox. A support file goes wherever the user chooses, which
  the save dialog grants.
- **Flathub shows these permissions on the app's page.** They must read as
  what they are: "read-only access to your home folder" and "network
  access". No wording of ours changes that.

### 3.3 The build

- **Source:** the GitHub release's `orbok-<version>.tar.gz`, by URL and
  `sha256`. It is the same archive the AUR's `orbok` builds from.
- **Rust dependencies:** `cargo-sources.json`, generated from `Cargo.lock`
  by `flatpak-cargo-generator`. Flathub builds offline.
- **Runtime:** the current `org.freedesktop.Platform`, with the
  `rust-stable` SDK extension. It must meet the MSRV (1.91); check the
  extension's version, do not assume it.
- **The manifest is kept in this repository** (`packaging/flatpak/`), next
  to the AUR recipes. The Flathub repository receives a copy with each
  release, so this repository stays the reviewed source of truth.

### 3.4 The metadata (AppStream)

`io.github.nabbisen.orbok.metainfo.xml`:
- the name, a summary and a description in English and Japanese, taken
  from the Store listing the owner approved;
- screenshots (the Store's, served from a tagged commit's raw URL);
- the releases, from the CHANGELOG;
- OARS content rating: none;
- the license (Apache-2.0), the homepage and the issue tracker.

**No email address.** `update_contact` is optional and is left out. The
developer is `nabbisen`.

The summary, new copy (AppStream wants about 35 characters), is
**approved by the owner, 2026-10-21:** "Search your documents by meaning" /
「意味から文書を検索」. Every other sentence is already approved.

### 3.5 Publishing

1. **First submission:** a pull request to `flathub/flathub`, opened by the
   owner from their GitHub account (Owner Task 006). Flathub's reviewers
   check the sandbox and the metadata. After acceptance, Flathub creates
   `flathub/io.github.nabbisen.orbok` and gives the owner maintainer
   access.
2. **Each release:** the release workflow (Task 133) gains two jobs, on
   the same pattern as the AUR's:
   - **`flatpak-check`:** builds the manifest from the release archive with
     `flatpak-builder` in CI, runs `flatpak-builder-lint` on the manifest
     and the metadata, and starts orbok's `--version` and `--check`
     inside the sandbox;
   - **`flathub-publish`:** `environment: flathub`, with the owner as
     required reviewer, the same guard as `aur` (it refuses without a
     reviewer rule). It opens a pull request on
     `flathub/io.github.nabbisen.orbok` with the new manifest and
     `cargo-sources.json`. Flathub builds that pull request, and **the
     owner merges it.**

   Flathub's external-data checker cannot regenerate
   `cargo-sources.json`, so it is not used.
3. **The token** that opens the pull request is fine-grained, scoped to
   `flathub/io.github.nabbisen.orbok` only, and stored only in the
   `flathub` environment.

## 4. What does not change

- **orbok's behaviour.** Nothing in the app branches on "am I in a
  Flatpak", except where §5 measures that it must.
- **The AUR packages.** They change only the renamed desktop entry and
  icon files (§3.1).
- **The Windows and Store packaging.**

## 5. To measure before acceptance closes (the implementing task)

1. **The folder picker inside the sandbox.**
   - For a folder under the home folder, does the FileChooser portal
     return the **real path** (the app can already read it), or a
     document-portal path (`/run/user/<uid>/doc/…`)?
   - Outside the home folder, is the document-portal path stable across
     restarts, and does it keep working?
   - A document-portal path for a folder inside an added one would defeat
     RFC-064's overlap check, which compares canonical paths. If real paths
     are not returned, report it before any workaround.
2. **A typed path** under the home folder works. Outside it, orbok says
   plainly that it cannot reach the path, and does not fail silently.
3. **Opening a result and its folder** goes through the OpenURI portal
   (`xdg-open` inside the runtime).
4. **The support file:** the save dialog's path is writable.
5. **Japanese text renders.** Host fonts reach the sandbox; check with no
   CJK font in the runtime.
6. **The portal probe (Task 132)** answers inside the sandbox.
7. **The model download, and search by meaning end to end,** inside the
   sandbox.

## 6. Consequences

- **A user who moves from the AUR package to the Flatpak** starts with a
  new profile, because the data lives in a different place. They add
  their folders again, and the user guide says so. orbok does not import
  another installation's profile: one profile, one owner of its data.
- **The desktop entry's file name changes for AUR users.** Menus pick it
  up automatically. A launcher pinned to the old entry has to be pinned
  again, which the CHANGELOG says.
- **One more approval per release** for the owner, in the same place as
  the AUR's.

## 7. Alternatives considered

- **Snap:** tied to one store and one vendor's infrastructure, and its
  confinement for a desktop app on non-Ubuntu systems is weaker. Flathub
  reaches more distributions with less.
- **AppImage:** no sandbox, no updates, and a binary built by us. That
  goes against the rule that no distributed binary comes from a developer
  machine, unless it gets its own build pipeline. Flathub's review and
  sandbox are worth more here.
- **Portal-only file access (no `home:ro`):** fewer permissions on paper.
  But every folder would become a document-portal path, typed paths would
  stop working, and RFC-064's canonical-path rule would be at risk. Left
  out unless §5.1 shows that portal paths are real paths.
- **Flathub's automatic update PRs (external-data checker):** cannot
  update the vendored Cargo sources (§3.5).

## 8. Acceptance criteria

1. orbok installs from Flathub as `io.github.nabbisen.orbok` and runs with
   exactly §3.2's permissions.
2. Every item in §5 is measured inside the sandbox and recorded, and any
   behaviour change it required is reviewed.
3. A release reaches Flathub only through the owner's approval and merge.
4. The AUR packages and the Flatpak share one ID, desktop entry and icon
   set.
5. The user guide has a Flathub install line, written after the first
   publication.
