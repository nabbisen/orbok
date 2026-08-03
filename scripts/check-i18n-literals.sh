#!/usr/bin/env bash
# check-i18n-literals.sh — RFC-052 i18n literal-copy gate.
#
# Fails if any designated UI/platform-integration file contains a raw
# English-looking string literal that isn't either (a) an already-catalogued
# `tr(locale, MessageKey::...)` call, or (b) an individually classified
# data/technical exception in scripts/i18n-literal-allowlist.txt.
#
# Heuristic, like the RFC-032 design-token gate: greps text, no parsing. It
# targets four shapes, chosen to match every violation found in the RFC-052
# Phase 1 inventory (review-request 132):
#   1. a raw literal passed directly to a known text-rendering call
#      (text(...), text_input(...), .set_title(...), job_progress(...));
#   2. a raw literal as an unwrap_or/unwrap_or_else fallback;
#   3. any quoted multi-word phrase anywhere in a designated file;
#   4. the ad-hoc "{}: {value}" label/value concatenation shape RFC-052 §4
#      rule 3 requires routed through a parameterized formatter instead.
# It will not catch every possible shape — no grep-based check can — a known
# gap (a single-word literal in an array-literal context, not a text-call
# argument) is recorded in review-request 132. But it is what turned up
# every other literal named in the Phase 1 inventory, and its self-check
# keeps it from silently scanning nothing.
set -euo pipefail

fail=0
flag() { echo "i18n-literal gate: $1"; fail=1; }

is_allowlisted() {
  local allowlist="$1" file="$2" literal="$3"
  grep -qF "$(printf '%s\t%s\t' "$file" "$literal")" "$allowlist"
}

# ── Heuristic literal-detection patterns ──────────────────────────────────
# check_file <allowlist-path> <file-to-scan>
#
# Patterns are tried most-specific first; a line already flagged by an
# earlier pattern is skipped by later ones (tracked in `seen`), so one
# violation produces exactly one finding even when its shape happens to
# match more than one pattern (e.g. a multi-word literal inside a
# text_input() call matches both Pattern 1 and Pattern 3).
check_file() {
  local allowlist="$1" file="$2"
  local -A seen=()

  # Pattern 1: raw literal passed directly to a text-rendering call.
  while IFS=: read -r lineno content; do
    [ -n "$lineno" ] || continue
    literal=$(sed -n "${lineno}p" "$file" | grep -oP '(?<=")[^"]*(?=")' | head -1)
    [ -n "$literal" ] || continue
    seen["$lineno"]=1
    if ! is_allowlisted "$allowlist" "$file" "$literal"; then
      flag "$file:$lineno: unrouted literal in text-rendering call: \"$literal\""
    fi
  done < <(grep -nP '\b(text|text_input)\(\s*"[^"]*[a-zA-Z]{2,}' "$file" 2>/dev/null || true)

  while IFS=: read -r lineno _; do
    [ -n "$lineno" ] || continue
    literal=$(sed -n "${lineno}p" "$file" | grep -oP '(?<=\.set_title\(")[^"]*' | head -1)
    [ -n "$literal" ] || continue
    seen["$lineno"]=1
    if ! is_allowlisted "$allowlist" "$file" "$literal"; then
      flag "$file:$lineno: unrouted literal in .set_title(): \"$literal\""
    fi
  done < <(grep -nP '\.set_title\(\s*"[^"]*[a-zA-Z]{2,}' "$file" 2>/dev/null || true)

  while IFS=: read -r lineno _; do
    [ -n "$lineno" ] || continue
    literal=$(sed -n "${lineno}p" "$file" | grep -oP '(?<=job_progress\(tokens, ")[^"]*' | head -1)
    [ -n "$literal" ] || continue
    seen["$lineno"]=1
    if ! is_allowlisted "$allowlist" "$file" "$literal"; then
      flag "$file:$lineno: unrouted literal in job_progress(): \"$literal\""
    fi
  done < <(grep -nP 'job_progress\(tokens,\s*"[^"]*[a-zA-Z]{2,}' "$file" 2>/dev/null || true)

  # Pattern 2: raw literal as an unwrap_or/unwrap_or_else fallback.
  while IFS=: read -r lineno _; do
    [ -n "$lineno" ] || continue
    literal=$(sed -n "${lineno}p" "$file" | grep -oP '(?<=unwrap_or\(")[^"]*' | head -1)
    [ -n "$literal" ] || continue
    seen["$lineno"]=1
    if ! is_allowlisted "$allowlist" "$file" "$literal"; then
      flag "$file:$lineno: unrouted literal as unwrap_or fallback: \"$literal\""
    fi
  done < <(grep -nP 'unwrap_or\(\s*"[^"]*[a-zA-Z]{2,}' "$file" 2>/dev/null || true)

  # Pattern 3: any quoted multi-word phrase, anywhere (catches struct-field
  # assignments and other shapes patterns 1/2 don't cover, e.g. the
  # `friendly_message: "..."` case in state.rs). Skips lines patterns 1/2
  # already flagged on this same line.
  while IFS=: read -r lineno _; do
    [ -n "$lineno" ] || continue
    [ -z "${seen[$lineno]:-}" ] || continue
    trimmed=$(sed -n "${lineno}p" "$file" | sed -e 's/^[[:space:]]*//')
    case "$trimmed" in
      //*) continue ;;
    esac
    literal=$(sed -n "${lineno}p" "$file" | grep -oP '"[A-Za-z][a-zA-Z]*(?:[ ][a-zA-Z][a-zA-Z]*){1,}[^"]*"' | head -1 | tr -d '"')
    [ -n "$literal" ] || continue
    if ! is_allowlisted "$allowlist" "$file" "$literal"; then
      flag "$file:$lineno: unrouted multi-word literal: \"$literal\""
    fi
  done < <(grep -nP '"[A-Za-z][a-zA-Z]*(?:[ ][a-zA-Z][a-zA-Z]*){1,}[^"]*"' "$file" 2>/dev/null || true)

  # Pattern 4: ad-hoc "{}: {value}" label/value concatenation (RFC-052 §4
  # rule 3 — must route through a parameterized formatter instead).
  while IFS=: read -r lineno _; do
    [ -n "$lineno" ] || continue
    flag "$file:$lineno: ad-hoc \"{}: {value}\" label/value concatenation — needs a parameterized i18n formatter, not format!()"
  done < <(grep -nP '^\s*"\{\}: ' "$file" 2>/dev/null || true)
}

main() {
  cd "$(dirname "${BASH_SOURCE[0]}")/.."
  local allowlist="scripts/i18n-literal-allowlist.txt"

  # ── Discovery roots (HANDOFF-052 §2 item 2) ─────────────────────────────
  # Tracked files only. Deliberately excludes: the catalog itself
  # (crates/ui/src/i18n.rs and i18n/{en,ja}.rs — the destination, not a
  # source to classify), test code (crates/ui/src/tests.rs, tests/*.rs —
  # not production UI), and crates/ui/src/theme.rs (persisted-setting
  # identifiers only, verified zero display text — see review-request 132).
  local files
  files=$(git ls-files \
    'crates/ui/src/views.rs' \
    'crates/ui/src/views/*.rs' \
    'crates/ui/src/components.rs' \
    'crates/ui/src/shell.rs' \
    'crates/ui/src/notice.rs' \
    'crates/ui/src/a11y.rs' \
    'crates/ui/src/state.rs' \
    'crates/ui/src/state/*.rs' \
    'crates/app/src/main.rs' \
    'crates/app/src/diagnostics.rs' \
    2>/dev/null || true)

  # Self-check: a wrong root, a moved file, or an empty git index must fail
  # loudly rather than silently pass on nothing checked (Response 130 §3's
  # "a check must be able to detect that it is not checking anything",
  # applied here per Task 005 §3).
  local file_count
  file_count=$(echo "$files" | grep -c . || true)
  if [ "$file_count" -lt 8 ]; then
    flag "discovery yielded only $file_count file(s) — expected at least 8; check the discovery roots above"
    echo "i18n-literal gate: failed" >&2
    exit 1
  fi

  if [ ! -f "$allowlist" ]; then
    flag "allowlist file missing: $allowlist"
    echo "i18n-literal gate: failed" >&2
    exit 1
  fi

  for file in $files; do
    check_file "$allowlist" "$file"
  done

  if [ "$fail" -ne 0 ]; then
    echo "i18n-literal gate: failed" >&2
    exit 1
  fi

  echo "i18n-literal gate: ok"
}

# Only run the discovery-rooted scan when executed directly. The self-test
# (check-i18n-literals.test.sh) sources this file to reuse check_file()/
# is_allowlisted() against fixture files instead, without triggering the
# real repository's discovery+self-check.
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  main
fi
