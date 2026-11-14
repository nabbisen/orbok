#!/usr/bin/env bash
# Task 148 §2 / Review 325 §4.5: the one reusable session for a real-UI
# GUI pass (`.git-exclude/rules/project-instructions-gui-common.md`'s
# "Reuse the setup; don't rebuild it" rule). Most "does it look right"
# questions no longer need this at all -- see
# `crates/ui/src/tests/task148_layout_matrix.rs`'s `snapshot` helper,
# which renders headless, with no window and no compositor. Use this
# script only for what headless rendering cannot show: font rendering,
# the window's own title and icon, real input.
#
# Three subcommands, so a pass that needs to drive input between launch
# and capture can (Review 325 §4.5 item 4 -- the one-shot
# launch-then-capture-then-teardown shape could not do this):
#
#   scripts/gui-session.sh start   <task> <width> <height> --host-resize -- <binary> [args...]
#   scripts/gui-session.sh capture <task> <capture-name.png>
#   scripts/gui-session.sh stop    <task>
#
#   task           Identifies the running session across the three
#                  calls, and names the evidence subdirectory: a capture
#                  lands at .git-exclude/evidence/<task>/<name.png>.
#   width, height  The app's requested logical window size in pixels.
#   binary         The GUI binary to launch (resolved to absolute before
#                  use).
#
# Run every call from the repository root. Anything already set in the
# caller's own environment (ORBOK_THEME, for example) still reaches the
# binary -- `start` overrides only the four directories named below.
#
# ── The rule this script's own sizing touches, quoted correctly ────────
#
# The owner's rule file says, of any window:
#
#   "Obtain it at launch -- through the app's own window-size setting, or
#    inside a nested compositor sized for the purpose -- and never by
#    resizing or floating a running window: on a shared desktop that
#    rearranges the layout and the capture then exposes other sessions."
#
# Measured on niri 26.04: a nested niri cannot size its own window --
# its outer window is placed by the HOST's tiling layout, and the nested
# config's own `output "winit" { mode "WxH" }` is ignored. The only way
# found to size it is exactly what the rule forbids: floating and
# resizing that one window on the host, by its window id, right after it
# appears. That is a real, if narrow, conflict with the rule as written,
# not a license to read around it -- raised as
# `.git-exclude/tasks/owner/007-sizing-the-nested-compositor-window.md`,
# which the owner has not yet decided.
#
# **So `start` refuses to run without `--host-resize`,** naming that
# task, rather than silently doing the host-side float-and-resize or
# silently skipping it and producing a window of whatever size the host
# handed it. Passing `--host-resize` is this script's own explicit,
# visible opt-in -- never the default, and never implied by any other
# flag -- until the owner answers.
#
# ── What was verified to make the sizing work, once opted in ───────────
#
# - **The host's output scale changes the arithmetic.** `set-window-*`
#   takes the HOST's logical pixels; the nested compositor's own virtual
#   "winit" output then reports *its* logical size at scale 1.0, equal to
#   the outer window's *physical* pixels -- i.e.
#   `inner_logical = outer_logical_requested * host_scale` (confirmed: at
#   host scale 1.2, requesting an outer 1280x720 produced an inner
#   1536x864). This script reads the host's own first output's scale and
#   divides by it before requesting the outer size (at scale 1.0, the
#   common case, this is a no-op). Override with `GUI_SESSION_SCALE=<n>`
#   if the window lands on a different output than the first one listed.
# - **A size that still doesn't land is a hard failure, not a warning**
#   (the rule file's "Assert, then continue": a capture at the wrong
#   size is wrong evidence) -- `start` aborts if the achieved inner size
#   is off by more than a couple of pixels (rounding) from the request.
# - **Exactly one window must appear inside the session.** `start` fails
#   if zero or more than one window shows up, rather than silently
#   taking the first.
# - **The app's first frame is confirmed, not assumed,** with one
#   internal capture (discarded, never saved as evidence) checked for
#   more than a single flat colour before `start` returns -- not a
#   frame-sampling loop (one check, not a search for a value).
# - **Screenshot writes land asynchronously** (a few seconds, observed).
#   `capture` polls for the file rather than assuming it is there on
#   return.
# - **Killing the nested niri's pid tears down everything** -- the
#   spawned binary and the nested Wayland/IPC sockets go with it.
#
# ── Input, for the pass this session feeds (not performed here) ────────
#
# - **Keyboard**: `wtype` reaches a native Wayland window (niri advertises
#   the virtual-keyboard protocol). Confirm focus with
#   `NIRI_SOCKET=<inner socket, printed by start> niri msg -j focused-window`
#   before every keystroke -- another window can steal focus between
#   steps.
# - **Mouse**: `xdotool` only works through XWayland. Force it with
#   `env -u WAYLAND_DISPLAY DISPLAY=:N <command>` *inside* this session
#   (the nested compositor also runs its own XWayland, logged at startup
#   as "listening on X11 socket: :N" in the session's own niri log).
#   `niri msg` still owns geometry either way.

set -euo pipefail

if [[ ! -d .git-exclude || ! -f Cargo.toml ]]; then
  echo "gui-session.sh: run from the repository root (no .git-exclude/ or Cargo.toml here)" >&2
  exit 2
fi
for tool in niri jq python3; do
  command -v "$tool" >/dev/null || {
    echo "gui-session.sh: $tool is not on PATH" >&2
    exit 2
  }
done

state_dir=".git-exclude/gui-session-state"
mkdir -p "$state_dir"

state_file() { echo "$state_dir/$1.env"; }

load_state() {
  local task=$1 f
  f=$(state_file "$task")
  [[ -f $f ]] || {
    echo "gui-session.sh: no running session for '$task' (run 'start' first, or it already stopped)" >&2
    exit 1
  }
  # shellcheck source=/dev/null
  source "$f"
}

cmd=${1:-}
[[ -n $cmd ]] || {
  echo "usage: $0 {start|capture|stop} ..." >&2
  exit 2
}
shift

case "$cmd" in
start)
  if [[ $# -lt 5 ]]; then
    echo "usage: $0 start <task> <width> <height> --host-resize -- <binary> [args...]" >&2
    exit 2
  fi
  task=$1
  width=$2
  height=$3
  host_resize=0
  if [[ ${4:-} == --host-resize ]]; then
    host_resize=1
    shift 4
  else
    shift 3
  fi
  [[ ${1:-} == -- ]] && shift
  if [[ $host_resize -ne 1 ]]; then
    cat >&2 <<'EOF'
gui-session.sh: refusing to start without --host-resize.

Sizing the nested compositor's own window means floating and resizing it
on the host, which the owner's rule forbids for any window, and the
owner has not yet decided the exception this script's header describes.
See .git-exclude/tasks/owner/007-sizing-the-nested-compositor-window.md.

Most captures no longer need a real window at all -- see
crates/ui/src/tests/task148_layout_matrix.rs's `snapshot` helper.
EOF
    exit 1
  fi
  [[ $# -ge 1 ]] || {
    echo "gui-session.sh: no binary given" >&2
    exit 2
  }
  binary=$1
  shift
  binary_args=("$@")

  f=$(state_file "$task")
  [[ -f $f ]] && {
    echo "gui-session.sh: a session for '$task' is already recorded at $f -- stop it first" >&2
    exit 1
  }

  binary_abs=$(realpath "$binary" 2>/dev/null) || {
    echo "gui-session.sh: binary not found: $binary" >&2
    exit 2
  }
  [[ -x $binary_abs ]] || {
    echo "gui-session.sh: not executable: $binary_abs" >&2
    exit 2
  }

  scratch=$(mktemp -d "$state_dir/$task.XXXXXX")

  cat > "$scratch/niri.kdl" <<'EOF'
// One app, filling the nested output -- no gaps, borders or focus ring
// to crop out of a capture, no animation to land mid-frame.
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

  abort() {
    echo "gui-session.sh: $1" >&2
    kill "$niri_pid" 2>/dev/null || true
    rm -rf "$scratch"
    exit 1
  }

  host_window_id=""
  for _ in $(seq 1 50); do
    host_window_id=$(niri msg -j windows \
      | jq -r --argjson pid "$niri_pid" '[.[] | select(.pid == $pid)][0].id // empty')
    [[ -n $host_window_id ]] && break
    sleep 0.2
  done
  [[ -n $host_window_id ]] || abort "the nested niri's own window never appeared (see $scratch/niri.log)"

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
  [[ -n $inner_socket ]] || abort "the nested niri's own IPC socket never appeared"

  inner_window_count=""
  inner_window_id=""
  for _ in $(seq 1 50); do
    windows_json=$(NIRI_SOCKET="$inner_socket" niri msg -j windows)
    inner_window_count=$(echo "$windows_json" | jq -r 'length')
    [[ $inner_window_count -ge 1 ]] && break
    sleep 0.2
  done
  if [[ -z $inner_window_count || $inner_window_count -eq 0 ]]; then
    abort "$binary_abs never showed a window inside the nested session (see $scratch/niri.log)"
  fi
  if [[ $inner_window_count -ne 1 ]]; then
    abort "expected exactly one window inside the nested session, found $inner_window_count"
  fi
  inner_window_id=$(echo "$windows_json" | jq -r '.[0].id')

  actual_width=$(NIRI_SOCKET="$inner_socket" niri msg -j outputs | jq -r '[.[].logical.width][0] // "?"')
  actual_height=$(NIRI_SOCKET="$inner_socket" niri msg -j outputs | jq -r '[.[].logical.height][0] // "?"')
  size_off=$(awk -v w="$width" -v h="$height" -v aw="$actual_width" -v ah="$actual_height" \
    'BEGIN { print (aw < w-2 || aw > w+2 || ah < h-2 || ah > h+2) ? "yes" : "no" }')
  if [[ $size_off == yes ]]; then
    abort "requested ${width}x${height} but the nested output landed at ${actual_width}x${actual_height} (host scale $host_scale) -- a capture at the wrong size is wrong evidence"
  fi

  # The app's first frame, confirmed rather than assumed: one capture,
  # discarded, checked for more than a single flat colour.
  probe_path="$scratch/first-frame-probe.png"
  NIRI_SOCKET="$inner_socket" niri msg action screenshot-window \
    --id "$inner_window_id" --path "$probe_path" >/dev/null
  for _ in $(seq 1 50); do
    [[ -s $probe_path ]] && break
    sleep 0.2
  done
  [[ -s $probe_path ]] || abort "the first-frame probe screenshot never appeared"
  if ! python3 -c "
import sys
from PIL import Image
img = Image.open('$probe_path').convert('RGB')
colors = img.getcolors(maxcolors=2)
sys.exit(0 if colors is None or len(colors) > 1 else 1)
" 2>/dev/null; then
    abort "the window's first frame is a single flat colour -- nothing has rendered yet"
  fi
  rm -f "$probe_path"

  {
    echo "niri_pid=$niri_pid"
    echo "scratch=$scratch"
    echo "inner_socket=$inner_socket"
    echo "inner_window_id=$inner_window_id"
    echo "host_window_id=$host_window_id"
    echo "task=$task"
  } > "$f"

  echo "gui-session.sh: started '$task' -- inner socket $inner_socket, window $inner_window_id, ${actual_width}x${actual_height}"
  ;;

capture)
  [[ $# -eq 2 ]] || {
    echo "usage: $0 capture <task> <capture-name.png>" >&2
    exit 2
  }
  task=$1
  capture_name=$2
  load_state "$task"

  evidence_dir="$(pwd)/.git-exclude/evidence/$task"
  mkdir -p "$evidence_dir"
  capture_path="$evidence_dir/$capture_name"

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
  ;;

stop)
  [[ $# -eq 1 ]] || {
    echo "usage: $0 stop <task>" >&2
    exit 2
  }
  task=$1
  load_state "$task"

  status=0
  if kill -0 "$niri_pid" 2>/dev/null; then
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

  if [[ $status -eq 0 ]]; then
    rm -rf "$scratch"
    rm -f "$(state_file "$task")"
    echo "gui-session.sh: stopped '$task'"
  else
    echo "gui-session.sh: kept $scratch and $(state_file "$task") for inspection" >&2
  fi
  exit "$status"
  ;;

*)
  echo "usage: $0 {start|capture|stop} ..." >&2
  exit 2
  ;;
esac
