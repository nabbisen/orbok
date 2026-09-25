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
  # Task 106: a row that holds a control wraps. A control that a narrow
  # window pushes past the edge cannot be reached, so every `hrow![..]` whose
  # children include a button, input or chip must end in `.wrap()` (after its
  # `.spacing(..)`), and a builder (`let mut x = hrow![..]`) must be wrapped
  # where it is used (`x.wrap()`). A row that must not wrap says why in a
  # `// no-wrap: <reason>` comment on the line above.
  # Task 118: a setting is shown by `components::choice` (one of several) or
  # `components::switch` (on/off), never by a button that is disabled -- or
  # otherwise treated differently -- because its value is the current one. The
  # old shape was `if *candidate != state.x { b = b.on_press(..) }`: the current
  # option had no press and was drawn disabled. This catches that shape (an `if`
  # comparing with `!=` whose braces hold an `.on_press(`) in the view files. It
  # cannot see a hand-built variant that is not written this way (a `match`, a
  # helper, `on_press_maybe` fed by a comparison); the standard itself is the
  # comment block in `components.rs`.
  local by_comparison
  by_comparison=$(presses_decided_by_comparison "${files[@]}")
  if [ -n "$by_comparison" ]; then
    echo "$by_comparison"
    flag "a button's press is decided by comparing with the current value — use components::choice (a choice) or components::switch (on/off) ($(echo "$by_comparison" | grep -c .) found)"
  fi

  local unwrapped
  unwrapped=$(control_rows_without_wrap "${files[@]}")
  if [ -n "$unwrapped" ]; then
    echo "$unwrapped"
    flag "hrow![ holds a control but does not wrap — add .wrap() after .spacing(..) (or // no-wrap: reason) ($(echo "$unwrapped" | grep -c .) found)"
  fi
}

# presses_decided_by_comparison <file...> — prints file:line for each
# `if <expr> != <expr> {` whose block contains `.on_press(` (Task 118).
presses_decided_by_comparison() {
  perl -0777 -ne '
    my $src = $_;
    while ($src =~ /\bif\s+[^{};]*?!=[^{};]*\{([^{}]{0,200})\}/g) {
      my $block = $1;
      my $line = (substr($src, 0, $-[0]) =~ tr/\n//) + 1;
      print "$ARGV:$line: if .. != .. { .. .on_press(..) }\n" if $block =~ /\.on_press\(/;
    }
  ' "$@"
}

# control_rows_without_wrap <file...> — prints file:line for each `hrow![` whose
# children include a control and whose method chain has no `.wrap(`; and for
# each `let mut NAME = hrow![` builder whose NAME is never `.wrap()`ped in the
# file. Text-only rows are out of the rule.
control_rows_without_wrap() {
  perl -0777 -ne '
    my $src = $_;
    my $control = qr/\bbutton\(|components::(?:ghost|primary|secondary|danger|chip|icon_primary|icon_secondary|filter_chip|danger_action|choice|switch)\b|text_input\(|pick_list\(|checkbox\(|toggler\(|\b\w*(?:_btn|_button|_input)\b|\bsubmit\b/;
    while ($src =~ /(?<![A-Za-z0-9_])hrow!\[/g) {
      my $start = $-[0];
      my $pos = pos($src);
      my $bodystart = $pos;
      my $depth = 1;
      while ($depth > 0 && $pos < length $src) {
        my $c = substr($src, $pos, 1);
        $depth++ if $c eq "[" || $c eq "(" || $c eq "{";
        $depth-- if $c eq "]" || $c eq ")" || $c eq "}";
        $pos++;
      }
      my $body = substr($src, $bodystart, $pos - $bodystart - 1);
      my $line = (substr($src, 0, $start) =~ tr/\n//) + 1;
      my $before = substr($src, 0, $start);
      next if $before =~ /\/\/ no-wrap:[^\n]*\n\s*(?:[^\n]*=\s*|return\s+)?$/;
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
      my $name = ($before =~ /let\s+mut\s+(\w+)\s*=\s*$/) ? $1 : undef;
      if (defined $name) {
        # A builder: pushes happen later, so judge the whole file: it must be
        # wrapped where used. (`x.wrap()` or `x)` passed to `.wrap()`.)
        print "$ARGV:$line: builder $name never wrapped\n" if $src !~ /\b$name\s*(?:\.spacing\((?:[^()]|\([^()]*\))*\)\s*)?\.wrap\(/;
      } elsif ($body =~ $control && $chain !~ /\.wrap\(/) {
        print "$ARGV:$line: hrow![ with a control, no .wrap(\n";
      }
    }
  ' "$@"
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
