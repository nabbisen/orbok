#!/usr/bin/env bash
# gui-session.test.sh — regression test for gui-session.sh (Task 148 §2,
# Review 325 §4.5). Needs a running niri session, jq, python3 (with
# Pillow) and alacritty (used only as a stand-in GUI binary -- any
# window-producing program would do); skips, rather than failing, when
# any of those is missing (a developer machine without niri, or a CI
# runner with no display) so this is safe to leave in place.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

if ! command -v niri >/dev/null || ! command -v jq >/dev/null \
  || ! command -v python3 >/dev/null || ! command -v alacritty >/dev/null \
  || [ -z "${WAYLAND_DISPLAY:-}" ] || ! niri msg -j windows >/dev/null 2>&1 \
  || ! python3 -c "import PIL" >/dev/null 2>&1; then
  echo "gui-session.test.sh: skipped (needs niri, jq, python3+Pillow, alacritty and a running niri session)"
  exit 0
fi

fail=0
task="gui-session-selftest-$$"
evidence_dir="$repo_root/.git-exclude/evidence/$task"
state_file="$repo_root/.git-exclude/gui-session-state/$task.env"
cleanup() {
  (cd "$repo_root" && bash scripts/gui-session.sh stop "$task") >/dev/null 2>&1 || true
  rm -rf "$evidence_dir"
}
trap cleanup EXIT

run() {
  (cd "$repo_root" && timeout 30 bash scripts/gui-session.sh "$@")
}

# (a) Without --host-resize, start refuses, naming Owner Task 007 -- the
# default must be the safe one.
out=$(run start "$task" 1280 720 -- "$(command -v alacritty)" 2>&1) && status=pass || status=fail
if [ "$status" = pass ]; then
  echo "FAIL: (a) start without --host-resize must not succeed" >&2
  fail=1
elif ! echo "$out" | grep -qF "owner/007-sizing-the-nested-compositor-window.md"; then
  echo "FAIL: (a) the refusal must name Owner Task 007:" >&2
  echo "$out" >&2
  fail=1
else
  echo "ok: (a) start refuses without --host-resize, naming Owner Task 007"
fi

# (b) With --host-resize, a real binary starts, lands at the requested
# size, and a capture through it succeeds.
out=$(run start "$task" 1280 720 --host-resize -- "$(command -v alacritty)" 2>&1) \
  && status=pass || status=fail
if [ "$status" != pass ]; then
  echo "FAIL: (b) start --host-resize should succeed on a real binary:" >&2
  echo "$out" >&2
  fail=1
else
  echo "ok: (b) start --host-resize reports success"
fi
if ! echo "$out" | grep -qE '1280x720'; then
  echo "FAIL: (b) start did not report landing at the requested size:" >&2
  echo "$out" >&2
  fail=1
else
  echo "ok: (b) the nested session landed at the requested logical size"
fi

out=$(run capture "$task" a.png 2>&1) && status=pass || status=fail
capture_path="$evidence_dir/a.png"
if [ "$status" != pass ] || [ ! -s "$capture_path" ]; then
  echo "FAIL: (b) capture should produce a non-empty file:" >&2
  echo "$out" >&2
  fail=1
else
  echo "ok: (b) capture produced a non-empty file"
fi

# (c) stop tears everything down: the recorded niri pid is gone, and its
# state/scratch files are removed.
niri_pid=$(grep '^niri_pid=' "$state_file" | cut -d= -f2)
run stop "$task" >/dev/null 2>&1
if kill -0 "$niri_pid" 2>/dev/null; then
  echo "FAIL: (c) niri pid $niri_pid is still running after stop" >&2
  fail=1
else
  echo "ok: (c) the nested niri was torn down"
fi
if [ -f "$state_file" ]; then
  echo "FAIL: (c) the state file was not removed" >&2
  fail=1
else
  echo "ok: (c) the session's state file was removed"
fi

# (d) Not vacuous: capture/stop against a task that was never started
# fail early, naming the problem, rather than crashing or silently
# succeeding.
out=$(run capture "no-such-task-$$" x.png 2>&1) && status=pass || status=fail
if [ "$status" = pass ] || ! echo "$out" | grep -qF "no running session"; then
  echo "FAIL: (d) capture against an unstarted task must fail, naming the problem:" >&2
  echo "$out" >&2
  fail=1
else
  echo "ok: (d) capture against an unstarted task fails early"
fi

if [ "$fail" -eq 0 ]; then
  echo "gui-session.test.sh: ok"
  exit 0
fi
echo "gui-session.test.sh: failed" >&2
exit 1
