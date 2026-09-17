#!/usr/bin/env bash
# check-design-tokens.sh — RFC-032 design-token gate.
#
# Fails if any orbok-ui view/component module contains a literal font size,
# padding, array padding, non-zero spacing, radius, or hard-coded iced
# colour (RFC-052 §5's five named categories). The only sanctioned styling
# path is the Snora Design token bridge via `crate::theme` helpers and
# `tokens.spacing.*`/`tokens.radius.*` (cf. the snora lucide/token gateway
# rule).
#
# Heuristic, like the RFC-052 i18n-literal gate: greps text, no parsing.
set -euo pipefail

fail=0
flag() { echo "design-token gate: $1"; fail=1; }

# check_tokens <file...> — runs all five category checks against the given
# files. Used both by main() (real discovery roots) and the self-test
# (fixture files).
check_tokens() {
  local files=("$@")
  [ "${#files[@]}" -gt 0 ] || return 0

  # Literal text sizes: .size(12)   (allow .size(theme::...), .size(var))
  if grep -nE '\.size\([0-9]' "${files[@]}"; then
    flag "literal text size — use theme::{body,meta,...}"
  fi
  # Literal bare paddings: .padding(10)
  if grep -nE '\.padding\([0-9]' "${files[@]}"; then
    flag "literal padding — use tokens.spacing.*"
  fi
  # Literal array paddings: Padding::from([12.0, 16.0])
  if grep -nE 'Padding::from\(\[[0-9.]' "${files[@]}"; then
    flag "literal array padding — use tokens.spacing.*"
  fi
  # Non-zero literal spacing: .spacing(8)   (spacing(0) is an allowed
  # structural zero — RFC-052 §5 requires removing *redundant* zeros, not
  # banning the sole case where zero is structurally meaningful).
  if grep -nE '\.spacing\([1-9]' "${files[@]}"; then
    flag "literal spacing — use tokens.spacing.*"
  fi
  # Literal corner radius: .rounded(12) / Radius::from(12.0) — the fifth
  # RFC-052 §5 category, missing from this gate until now (Task 005).
  if grep -nE '\.rounded\([0-9]|Radius::from\([0-9]' "${files[@]}"; then
    flag "literal radius — use tokens.radius.*"
  fi
  # Hard-coded colours.
  if grep -nE 'iced::Color|Color::from_rgb|from_rgba' "${files[@]}"; then
    flag "literal colour — use palette roles via the token bridge"
  fi
  # Task 072: every horizontal row declares its vertical alignment. A bare
  # `row![...]` aligns its children to the top, so an icon sits above its
  # label and a button hangs below the input beside it. Use `hrow![...]`
  # (centred), or `row![...].align_y(..)` for a deliberate choice. Unlike
  # the categories above this spans lines, so it balances the macro's
  # brackets and reads the method chain that follows it.
  local bare_rows
  bare_rows=$(bare_rows_without_alignment "${files[@]}")
  if [ -n "$bare_rows" ]; then
    echo "$bare_rows"
    flag "row![ without .align_y( — use hrow![..] (centred) or row![..].align_y(..) ($(echo "$bare_rows" | grep -c .) found)"
  fi
}

# bare_rows_without_alignment <file...> — prints file:line for each `row![`
# whose expression (the balanced macro plus its `.method(..)` chain) has no
# `.align_y(`. `hrow![` does not match: the word boundary excludes it.
bare_rows_without_alignment() {
  perl -0777 -ne '
    my $src = $_;
    while ($src =~ /(?<![A-Za-z0-9_])row!\[/g) {
      my $start = $-[0];
      my $pos = pos($src);
      my $depth = 1;
      while ($depth > 0 && $pos < length $src) {
        my $c = substr($src, $pos, 1);
        $depth++ if $c eq "[" || $c eq "(" || $c eq "{";
        $depth-- if $c eq "]" || $c eq ")" || $c eq "}";
        $pos++;
      }
      my $chain = "";
      while (substr($src, $pos) =~ /^(\s*\.\s*[A-Za-z_][A-Za-z0-9_]*\s*)/) {
        $chain .= $1;
        $pos += length $1;
        if (substr($src, $pos, 1) eq "(") {
          my $d = 0;
          do {
            my $c = substr($src, $pos, 1);
            $d++ if $c eq "(";
            $d-- if $c eq ")";
            $chain .= $c;
            $pos++;
          } while ($d > 0 && $pos < length $src);
        }
      }
      if ($chain !~ /\.align_y\(/) {
        my $line = (substr($src, 0, $start) =~ tr/\n//) + 1;
        print "$ARGV:$line: row![ without .align_y(\n";
      }
    }
  ' "$@"
}

main() {
  cd "$(dirname "${BASH_SOURCE[0]}")/.."

  # View/component modules that must be fully token-driven.
  local files_str
  files_str=$(git ls-files 'crates/ui/src/views.rs' 'crates/ui/src/views/*.rs' \
                           'crates/ui/src/shell.rs' 'crates/ui/src/components.rs' \
                           2>/dev/null || true)

  # Self-check: no silent fallback to an unfiltered disk `ls` (the prior
  # behavior — if `git ls-files` ever returned empty, the gate would
  # silently scan a working-tree glob that could include untracked files,
  # or, if that also matched nothing, feed grep an empty file list and
  # report a spurious pass). A wrong root or empty git index must fail
  # loudly instead (Task 005 §3 / Response 130 §3).
  local file_count
  file_count=$(echo "$files_str" | grep -c . || true)
  if [ "$file_count" -lt 4 ]; then
    flag "discovery yielded only $file_count file(s) — expected at least 4 (views.rs, views/wizard.rs, shell.rs, components.rs)"
    echo "design-token gate: failed" >&2
    exit 1
  fi

  # shellcheck disable=SC2206
  local files=($files_str)
  check_tokens "${files[@]}"

  if [ "$fail" -ne 0 ]; then
    echo "FAIL: magic styling values found in view modules (RFC-032)."
    exit 1
  fi
  echo "design-token gate: ok"
}

# Only run the discovery-rooted scan when executed directly. The self-test
# (check-design-tokens.test.sh) sources this file to reuse check_tokens()
# against fixture files instead.
if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  main
fi
