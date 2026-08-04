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

# ── Discovery contract (RFC-052 §6) ────────────────────────────────────────
#
# "The checker discovers tracked files under designated UI and
# platform-integration directories and compares that set exactly with a
# classified allowlist... A new unclassified tracked file fails the gate."
#
# Designated directories:
#   - crates/ui/src/**  (the whole UI crate — RFC-027 gives it zero fs/db
#     access, so everything here is potential display surface), except two
#     subdirectories excluded by directory, not by individual file:
#       - i18n/   the catalog's own destination files, not a source to
#                  classify
#       - tests/  test code, not production UI
#   - crates/app/src/*.rs  (flat, top-level only). Platform-integration
#     files — native dialogs, the diagnostics preview — live here.
#     Subdirectories (bootstrap/, runtime_context/, runtime_storage/) are
#     backend/filesystem/database glue with no rendering path, out of
#     RFC-027's UI boundary by architecture rather than by checker
#     classification.
#
# Every file `git ls-files` returns under those roots must appear in
# exactly one of SCAN_FILES (below, scanned by check_file) or
# EXCLUDED_FILES (below, individually reasoned). A file in neither bucket
# fails the gate by name, so a new file cannot go unexamined the way an
# enumerated allowlist of patterns could miss it (Review 141 §3(c)).
discover_files() {
  git ls-files -- \
    'crates/ui/src/*.rs' ':!crates/ui/src/i18n/*' ':!crates/ui/src/tests/*' \
    'crates/app/src/*.rs' ':!crates/app/src/*/*' \
    2>/dev/null | sort -u
}

SCAN_FILES=(
  crates/ui/src/a11y.rs
  crates/ui/src/components.rs
  crates/ui/src/notice.rs
  crates/ui/src/shell.rs
  crates/ui/src/state.rs
  crates/ui/src/state/location.rs
  crates/ui/src/state/model_consent.rs
  crates/ui/src/state/search.rs
  crates/ui/src/views.rs
  crates/ui/src/views/wizard.rs
  crates/app/src/diagnostics.rs
  crates/app/src/main.rs
)

# file → reason, one entry per non-scanned file discovery yields today.
# Verified in full (not sampled) via
#   grep -nP '"[A-Za-z][a-zA-Z]*(?:[ ][a-zA-Z][a-zA-Z]*){1,}[^"]*"' <file>
# before writing each reason.
declare -A EXCLUDED_FILES=(
  ["crates/ui/src/i18n.rs"]="the catalog itself — destination, not a source to classify"
  ["crates/ui/src/lib.rs"]="crate root: module declarations and re-exports only, verified zero display text"
  ["crates/ui/src/tests.rs"]="test module router, not production UI"
  ["crates/ui/src/theme.rs"]="persisted-setting serialization identifiers only (\"system\"/\"light\"/\"dark\"/etc.), verified zero display text in full (review-request 132 §3); display text for themes already routes through MessageKey::Theme*"
  ["crates/app/src/bootstrap.rs"]="module declarations only, verified zero display text"
  ["crates/app/src/download.rs"]="developer-facing tracing/log and panic/expect strings only, verified in full; errors reach users only via typed UserNotice variants routed through the catalog, never shown raw"
  ["crates/app/src/history.rs"]="developer-facing tracing/log strings only, verified in full"
  ["crates/app/src/lib.rs"]="crate root: module declarations only, verified zero display text"
  ["crates/app/src/model_flow.rs"]="developer-facing tracing/log/panic/assert strings and inline #[cfg(test)] module content only, verified in full"
  ["crates/app/src/physical_identity.rs"]="io::Error Display strings only — a backend error type never rendered raw to users (RFC-052 §3), verified in full"
  ["crates/app/src/runtime_context.rs"]="Display impl strings for a backend error type (RFC-049), same RFC-052 §3 exemption as physical_identity.rs, verified in full"
  ["crates/app/src/runtime_isolation_tests.rs"]="test-only module (#[cfg(test)] in main.rs), not production code"
  ["crates/app/src/runtime_storage.rs"]="Display impl strings for a backend error type, same exemption as runtime_context.rs, verified in full"
  ["crates/app/src/settings.rs"]="io::Error Display string only, verified in full"
)

is_scan_file() {
  local file="$1" f
  for f in "${SCAN_FILES[@]}"; do
    [ "$f" = "$file" ] && return 0
  done
  return 1
}

# classify_file <path>
# Echoes "scan", "exclude", or "unclassified". Pure function of SCAN_FILES/
# EXCLUDED_FILES — takes no git state, so the self-test can exercise the
# "new file matches neither bucket" case with a fabricated path (Review 141
# §3(c)'s own hypothetical, "crates/ui/src/panels.rs") without needing a
# throwaway git repository.
classify_file() {
  local file="$1"
  if is_scan_file "$file"; then
    echo scan
  elif [ -n "${EXCLUDED_FILES[$file]:-}" ]; then
    echo exclude
  else
    echo unclassified
  fi
}

main() {
  cd "$(dirname "${BASH_SOURCE[0]}")/.."
  local allowlist="scripts/i18n-literal-allowlist.txt"

  if [ ! -f "$allowlist" ]; then
    flag "allowlist file missing: $allowlist"
    echo "i18n-literal gate: failed" >&2
    exit 1
  fi

  local discovered
  discovered=$(discover_files)

  # Self-check: a wrong root or an empty git index must fail loudly rather
  # than silently pass on nothing checked (Response 130 §3's "a check must
  # be able to detect that it is not checking anything"). The exhaustive
  # per-file classification below is the primary guarantee; this floor is
  # defense-in-depth against discovery breaking entirely.
  local discovered_count
  discovered_count=$(echo "$discovered" | grep -c . || true)
  if [ "$discovered_count" -lt 20 ]; then
    flag "discovery yielded only $discovered_count file(s) — expected at least 20; check the designated directories"
    echo "i18n-literal gate: failed" >&2
    exit 1
  fi

  while IFS= read -r file; do
    [ -n "$file" ] || continue
    case "$(classify_file "$file")" in
      scan) check_file "$allowlist" "$file" ;;
      exclude) : ;;
      unclassified)
        flag "$file: new tracked file under a designated directory with no classification — add it to SCAN_FILES or EXCLUDED_FILES (with a reason) in scripts/check-i18n-literals.sh"
        ;;
    esac
  done <<<"$discovered"

  if [ "$fail" -ne 0 ]; then
    echo "i18n-literal gate: failed" >&2
    exit 1
  fi

  echo "i18n-literal gate: ok"
}

# Only run the discovery-rooted scan when executed directly. The self-test
# (check-i18n-literals.test.sh) sources this file to reuse check_file()/
# is_allowlisted()/classify_file() against fixture files and fabricated
# paths instead, without triggering the real repository's discovery+scan.
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  main
fi
