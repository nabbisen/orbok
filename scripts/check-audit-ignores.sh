#!/usr/bin/env bash
# check-audit-ignores.sh — every ignore in .cargo/audit.toml must still be
# true (Task 044).
#
# `cargo audit --deny warnings` answers "is every advisory in the lockfile
# fixed or ignored?" It never asks whether an ignore still matches anything:
# a fix publishes, a crate leaves the graph, and the entry silently outlives
# its reason. This gate fails when that happens, and when the scanner is no
# longer configured to see every advisory class.
#
# Mechanism: cargo-audit 0.22.2's JSON output does not list ignored
# advisories, so the lockfile is audited a second time from an empty
# directory via `--file`. The project's .cargo/audit.toml is only discovered
# relative to the working directory, so that run sees no ignores -- without
# editing any config. `--no-fetch` reuses the advisory DB the preceding
# `cargo audit` step already fetched, so both runs judge the same data.
#
# Overridable for the self-test: AUDIT_TOML, CI_YML, LOCKFILE, CARGO_AUDIT.
set -euo pipefail

audit_toml="${AUDIT_TOML:-.cargo/audit.toml}"
ci_yml="${CI_YML:-.github/workflows/ci.yml}"
lockfile="${LOCKFILE:-Cargo.lock}"
cargo_audit="${CARGO_AUDIT:-cargo audit}"

fail=0
flag() {
  echo "check-audit-ignores: $1" >&2
  fail=1
}

for f in "$audit_toml" "$ci_yml" "$lockfile"; do
  [ -f "$f" ] || { echo "check-audit-ignores: missing $f" >&2; exit 1; }
done

# ── 1. The scanner sees every advisory class ────────────────────────────
# --deny warnings is what makes unmaintained/unsound/yanked fatal; without
# it every ignore below is decorative.
if ! grep -qE '^[[:space:]]*(-[[:space:]]*)?run:[[:space:]]*cargo audit([[:space:]].*)?[[:space:]](--deny|-D)[[:space:]]+warnings([[:space:]]|$)' "$ci_yml"; then
  flag "$ci_yml has no 'run: cargo audit ... --deny warnings' step -- unsound/unmaintained/yanked advisories would not fail CI"
fi
# An absent informational_warnings key is the default (all classes). A
# present one must not narrow it.
if grep -qE '^[[:space:]]*informational_warnings[[:space:]]*=' "$audit_toml"; then
  line="$(grep -E '^[[:space:]]*informational_warnings[[:space:]]*=' "$audit_toml")"
  for class in unmaintained unsound; do
    grep -q "\"$class\"" <<<"$line" \
      || flag "$audit_toml sets informational_warnings without \"$class\" -- that class is no longer reported: $line"
  done
fi

# ── 2. Every ignore still matches an advisory in this lockfile ──────────
# id<TAB>first line of the comment block above it (ids sharing one block
# share its first line).
ignores="$(awk '
  /^[[:space:]]*#/ {
    text = $0; sub(/^[[:space:]]*#[[:space:]]*/, "", text)
    if (!in_block) { first = text; in_block = 1 }
    next
  }
  {
    in_block = 0
    line = $0; sub(/#.*/, "", line)
    while (match(line, /"RUSTSEC-[0-9]+-[0-9]+"/)) {
      print substr(line, RSTART + 1, RLENGTH - 2) "\t" first
      line = substr(line, RSTART + RLENGTH)
    }
  }
' "$audit_toml")"

if [ -n "$ignores" ]; then
  neutral="$(mktemp -d)"
  trap 'rm -rf "$neutral"' EXIT
  abs_lock="$(cd "$(dirname "$lockfile")" && pwd)/$(basename "$lockfile")"
  # cargo audit exits non-zero whenever it finds anything, which it will
  # here by design; only a missing or malformed report is an error.
  (cd "$neutral" && $cargo_audit --no-fetch --json --file "$abs_lock") \
    >"$neutral/report.json" 2>"$neutral/stderr" || true
  if ! jq -e '.lockfile' "$neutral/report.json" >/dev/null 2>&1; then
    echo "check-audit-ignores: could not audit $lockfile without ignores:" >&2
    cat "$neutral/stderr" >&2
    exit 1
  fi
  reported="$(jq -r '
    (.vulnerabilities.list[]?.advisory.id),
    (.warnings // {} | to_entries[] | .value[]? | .advisory.id // empty)
  ' "$neutral/report.json" | sort -u)"

  while IFS=$'\t' read -r id reason; do
    [ -n "$id" ] || continue
    if ! grep -qxF "$id" <<<"$reported"; then
      flag "$id is ignored but no longer reported against $lockfile (fixed, or its crate left the graph) -- delete it. Its reason was: ${reason:-(no comment)}"
    fi
  done <<<"$ignores"
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "check-audit-ignores: ok ($(grep -c . <<<"$ignores") ignores, each still reported)"
