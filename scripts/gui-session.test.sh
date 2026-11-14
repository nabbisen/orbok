#!/usr/bin/env bash
# gui-session.test.sh — regression test for gui-session.sh (Task 148 §2).
# Needs a running niri session, jq, and alacritty (used only as a stand-in
# GUI binary -- any window-producing program would do); skips, rather than
# failing, when any of those is missing (a developer machine without
# niri, or a CI runner with no display) so this is safe to leave in place.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

if ! command -v niri >/dev/null || ! command -v jq >/dev/null \
  || ! command -v alacritty >/dev/null || [ -z "${WAYLAND_DISPLAY:-}" ] \
  || ! niri msg -j windows >/dev/null 2>&1; then
  echo "gui-session.test.sh: skipped (needs niri, jq, alacritty and a running niri session)"
  exit 0
fi

fail=0
task="gui-session-selftest-$$"
evidence_dir="$repo_root/.git-exclude/evidence/$task"
trap 'rm -rf "$evidence_dir"' EXIT

run_session() {
  (cd "$repo_root" && timeout 30 bash scripts/gui-session.sh "$@")
}

# (a) A real binary: the session reports success, the capture exists and
# is a non-empty PNG, and the reported inner size matches what was asked.
out=$(run_session "$task" 1280 720 a.png "$(command -v alacritty)" 2>&1) \
  && status=pass || status=fail
if [ "$status" != pass ]; then
  echo "FAIL: (a) a real binary should produce a capture:" >&2
  echo "$out" >&2
  fail=1
else
  echo "ok: (a) the session reports success"
fi
capture="$evidence_dir/a.png"
if [ ! -s "$capture" ]; then
  echo "FAIL: (a) no capture at $capture" >&2
  fail=1
else
  echo "ok: (a) the capture file exists and is non-empty"
fi
if ! echo "$out" | grep -qE 'nested output reports 1280x720'; then
  echo "FAIL: (a) the session did not report landing at the requested size:" >&2
  echo "$out" >&2
  fail=1
else
  echo "ok: (a) the nested session actually landed at the requested logical size"
fi

# (b) Teardown: nothing the session launched is still running, and its
# scratch directory is gone.
if pgrep -f "niri -c .*gui-session" >/dev/null 2>&1; then
  echo "FAIL: (b) a nested niri from this test is still running" >&2
  fail=1
else
  echo "ok: (b) no nested niri was left running"
fi
if compgen -G "$repo_root/.git-exclude/gui-session.*" >/dev/null; then
  echo "FAIL: (b) a scratch directory was left behind" >&2
  fail=1
else
  echo "ok: (b) the scratch directory was cleaned up"
fi

# (c) Not vacuous: a binary that does not exist is a clear, early failure,
# not a silent pass or a confusing crash deep in the session setup.
out=$(run_session "$task" 1280 720 c.png /no/such/binary 2>&1) && status=pass || status=fail
if [ "$status" = pass ]; then
  echo "FAIL: (c) a missing binary must not report success" >&2
  fail=1
elif ! echo "$out" | grep -qF "binary not found"; then
  echo "FAIL: (c) a missing binary's error must name the problem:" >&2
  echo "$out" >&2
  fail=1
else
  echo "ok: (c) a missing binary fails early, naming the problem"
fi

if [ "$fail" -eq 0 ]; then
  echo "gui-session.test.sh: ok"
  exit 0
fi
echo "gui-session.test.sh: failed" >&2
exit 1
