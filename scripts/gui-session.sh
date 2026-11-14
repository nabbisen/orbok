#!/usr/bin/env bash
# Task 148 §2: the one reusable session for a real-UI GUI pass
# (`.git-exclude/rules/project-instructions-gui-common.md`'s "Reuse the
# setup; don't rebuild it" rule). Each pass calls this once instead of
# standing up a nested compositor and isolated profile by hand.
#
# What this does, in order: starts a nested niri compositor sized for the
# requested logical window size, with every directory the app reads
# (ORBOK_DATA_DIR, XDG_CONFIG_HOME, XDG_STATE_HOME, XDG_CACHE_HOME)
# isolated under a scratch directory inside `.git-exclude/`; launches one
# binary inside it; captures its window to the path given; tears the
# nested compositor down; confirms nothing from this run is left running.
#
# Usage:
#   scripts/gui-session.sh <task> <width> <height> <capture-name.png> <binary> [args...]
#
#   task           Evidence subdirectory: the capture lands at
#                  .git-exclude/evidence/<task>/<capture-name.png>.
#   width, height  The app's logical window size in pixels (e.g. 1280 720,
#                  or 450 1400 for the narrow-window matrix's own sizes --
#                  see `crates/ui/src/tests/task148_layout_matrix.rs`).
#   binary         The GUI binary to launch (an absolute or relative path;
#                  resolved to absolute before use).
#
# Run from the repository root. Anything already set in the caller's own
# environment (ORBOK_THEME, for example) still reaches the binary --
# only the four isolated directories above are overridden.
#
# ── What was verified to make this work (niri 26.04, this machine) ──────
#
# - **A nested niri's own OUTER window is still owned by the HOST
#   compositor's tiling layout**: `output "winit" { mode "WxH" }` in the
#   nested compositor's own config has no effect on its outer window
#   size (confirmed: it was silently ignored). The outer window must be
#   floated and explicitly sized on the HOST, exactly as
#   `project-instructions-gui-common.md`'s own guidance for any tiled
#   window: `niri msg action move-window-to-floating`, then
#   `set-window-width`/`set-window-height` -- never `xdotool windowsize`
#   (wrong units on a scaled output; see the same rule file).
# - **The host's output scale changes the arithmetic.** `set-window-*`
#   takes the HOST's logical pixels; the nested compositor's own virtual
#   "winit" output then reports *its* logical size at scale 1.0, equal to
#   the outer window's *physical* pixels -- i.e.
#   `inner_logical = outer_logical_requested * host_scale` (confirmed: at
#   host scale 1.2, requesting an outer 1280x720 produced an inner
#   1536x864). This script reads the host's own first output's scale and
#   divides by it before requesting the outer size, so the inner app
#   lands at the requested logical size regardless of host scale (at
#   scale 1.0, the common case -- most CI and most single-monitor
#   desktops -- this is a no-op). Override with `GUI_SESSION_SCALE=<n>`
#   if the window lands on a different output than the first one listed.
# - **Screenshot writes asynchronously.** `niri msg action
#   screenshot-window` returns before the PNG is on disk (observed delay:
#   a few seconds for a small window). This script polls for the file
#   rather than assuming it is there on return -- not a frame-sampling
#   loop (no repeated screenshots; it samples the filesystem for one
#   write to land).
# - **Killing the nested niri's PID is enough.** It takes the spawned
#   binary and the nested Wayland/IPC sockets down with it; nothing else
#   to tear down separately.
#
# ── Input, for the pass this session feeds (not performed here) ────────
#
# - **Keyboard**: `wtype` reaches a native Wayland window (niri advertises
#   the virtual-keyboard protocol). Confirm focus with
#   `NIRI_SOCKET=<inner socket> niri msg -j focused-window` before every
#   keystroke -- another window can steal focus between steps.
# - **Mouse**: `xdotool` only works through XWayland. Force it with
#   `env -u WAYLAND_DISPLAY DISPLAY=:N <binary>` *inside* this session
#   (the nested compositor also runs its own XWayland, logged at startup
#   as "listening on X11 socket: :N" in the capture's own niri log).
#   `niri msg` still owns geometry either way.

set -euo pipefail

if [[ $# -lt 5 ]]; then
  echo "usage: $0 <task> <width> <height> <capture-name.png> <binary> [args...]" >&2
  exit 2
fi

task=$1
width=$2
height=$3
capture_name=$4
binary=$5
shift 5
binary_args=("$@")

if [[ ! -d .git-exclude || ! -f Cargo.toml ]]; then
  echo "gui-session.sh: run from the repository root (no .git-exclude/ or Cargo.toml here)" >&2
  exit 2
fi
if ! command -v niri >/dev/null; then
  echo "gui-session.sh: niri is not on PATH" >&2
  exit 2
fi
if ! command -v jq >/dev/null; then
  echo "gui-session.sh: jq is not on PATH" >&2
  exit 2
fi
binary_abs=$(realpath "$binary" 2>/dev/null) || {
  echo "gui-session.sh: binary not found: $binary" >&2
  exit 2
}
[[ -x $binary_abs ]] || {
  echo "gui-session.sh: not executable: $binary_abs" >&2
  exit 2
}

evidence_dir="$(pwd)/.git-exclude/evidence/$task"
mkdir -p "$evidence_dir"
capture_path="$evidence_dir/$capture_name"

scratch=$(mktemp -d ".git-exclude/gui-session.XXXXXX")
cleanup_scratch=1
niri_pid=""

cleanup() {
  local status=$?
  if [[ -n $niri_pid ]] && kill -0 "$niri_pid" 2>/dev/null; then
    kill "$niri_pid" 2>/dev/null || true
    for _ in $(seq 1 20); do
      kill -0 "$niri_pid" 2>/dev/null || break
      sleep 0.2
    done
    if kill -0 "$niri_pid" 2>/dev/null; then
      kill -9 "$niri_pid" 2>/dev/null || true
      sleep 0.2
    fi
    if kill -0 "$niri_pid" 2>/dev/null; then
      echo "gui-session.sh: niri pid $niri_pid would not die -- left running, check by hand" >&2
      status=1
    fi
  fi
  if [[ $status -eq 0 && $cleanup_scratch -eq 1 ]]; then
    rm -rf "$scratch"
  elif [[ -d $scratch ]]; then
    echo "gui-session.sh: kept $scratch (niri.log inside it) for inspection" >&2
  fi
  exit "$status"
}
trap cleanup EXIT INT TERM

cat > "$scratch/niri.kdl" <<'EOF'
// Task 148 §2: one app, filling the nested output -- no gaps, borders or
// focus ring to crop out of a capture, no animation to land mid-frame.
layout {
    gaps 0
    default-column-width { proportion 1.0; }
    border {
        off
    }
    focus-ring {
        off
    }
}

animations {
    off
}

prefer-no-csd
EOF

data_dir="$scratch/data"
mkdir -p "$data_dir" "$scratch/config" "$scratch/state" "$scratch/cache"

ORBOK_DATA_DIR="$data_dir" \
XDG_CONFIG_HOME="$scratch/config" \
XDG_STATE_HOME="$scratch/state" \
XDG_CACHE_HOME="$scratch/cache" \
  niri -c "$scratch/niri.kdl" -- "$binary_abs" "${binary_args[@]}" \
  > "$scratch/niri.log" 2>&1 &
niri_pid=$!

host_window_id=""
for _ in $(seq 1 50); do
  host_window_id=$(niri msg -j windows \
    | jq -r --argjson pid "$niri_pid" '[.[] | select(.pid == $pid)][0].id // empty')
  [[ -n $host_window_id ]] && break
  sleep 0.2
done
[[ -n $host_window_id ]] || {
  echo "gui-session.sh: the nested niri's own window never appeared (see $scratch/niri.log)" >&2
  exit 1
}

host_scale=${GUI_SESSION_SCALE:-$(niri msg -j outputs | jq -r '[.[].logical.scale][0] // 1')}
outer_width=$(awk -v w="$width" -v s="$host_scale" 'BEGIN { printf "%d", (w / s) + 0.5 }')
outer_height=$(awk -v h="$height" -v s="$host_scale" 'BEGIN { printf "%d", (h / s) + 0.5 }')

niri msg action move-window-to-floating --id "$host_window_id" >/dev/null
niri msg action set-window-width --id "$host_window_id" "$outer_width" >/dev/null
niri msg action set-window-height --id "$host_window_id" "$outer_height" >/dev/null

inner_socket=""
for _ in $(seq 1 50); do
  inner_socket=$(find "/run/user/$(id -u)" -maxdepth 1 -name "niri.*.${niri_pid}.sock" -print -quit 2>/dev/null)
  [[ -n $inner_socket ]] && break
  sleep 0.2
done
[[ -n $inner_socket ]] || {
  echo "gui-session.sh: the nested niri's own IPC socket never appeared" >&2
  exit 1
}

inner_window_id=""
for _ in $(seq 1 50); do
  inner_window_id=$(NIRI_SOCKET="$inner_socket" niri msg -j windows | jq -r '.[0].id // empty')
  [[ -n $inner_window_id ]] && break
  sleep 0.2
done
[[ -n $inner_window_id ]] || {
  echo "gui-session.sh: $binary_abs never showed a window inside the nested session (see $scratch/niri.log)" >&2
  exit 1
}

actual_width=$(NIRI_SOCKET="$inner_socket" niri msg -j outputs | jq -r '[.[].logical.width][0] // "?"')
actual_height=$(NIRI_SOCKET="$inner_socket" niri msg -j outputs | jq -r '[.[].logical.height][0] // "?"')
echo "gui-session.sh: requested ${width}x${height}, nested output reports ${actual_width}x${actual_height} (host scale $host_scale)" >&2

NIRI_SOCKET="$inner_socket" niri msg action screenshot-window \
  --id "$inner_window_id" --path "$capture_path" >/dev/null

for _ in $(seq 1 50); do
  [[ -s $capture_path ]] && break
  sleep 0.2
done
[[ -s $capture_path ]] || {
  echo "gui-session.sh: screenshot never appeared at $capture_path" >&2
  exit 1
}

echo "gui-session.sh: captured $capture_path"
