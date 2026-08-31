#!/usr/bin/env bash
# Sweep `luxel check` over every pattern in library/ — the standing acceptance
# gate for engine / compiler / bytecode changes. Per pattern `check` does a
# compile, an LXBC serialize→deserialize→re-serialize round-trip (byte-identical
# required, plus the decoded program must render identically to the fresh
# compile — that is the device's execution path) and a 3-frame smoke run.
#
# Before the engine sweep it also lints `//#` control directives: one that is
# not adjacent to an export is silently ignored by the parser, so a pattern can
# ship a directive that does nothing at all (Gitea #179). See "directive lint"
# below for the exact rule.
#
# Every pattern runs on TWO rigs: `check`'s default 10x10 grid and an explicit
# 16x16. Grid size is load-bearing — patterns that hardcode a rig width or do
# pixelCount arithmetic go out of bounds on one shape and not the other — and
# "322/322 clean on both the default and 16x16 grids" is the phrasing every
# past sweep in UPDATES.md reports. This script is that loop, so nobody has to
# re-derive it from prose again.
#
# Run in the nix devshell:
#   nix develop -c tools/check-library.sh [dir]        (default dir: library)
#   GRIDS="default 16x16 8x32" nix develop -c tools/check-library.sh
#
# Prints a per-grid pass count, the failing file + engine stage/error for each
# failure, and exits non-zero if anything failed. Baseline: 322/322 on both.
# (corpus/ has its own richer harness — tools/corpus/report.mjs.)
set -euo pipefail
TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$TOOLS_DIR/.." && pwd)"
cd "$ROOT"

DIR="${1:-library}"
# Space-separated rig list; "default" means no --grid flag (luxel check's own
# 10x10), anything else is passed through as --grid WxH.
GRIDS="${GRIDS:-default 16x16}"

shopt -s nullglob
files=("$DIR"/*.js "$DIR"/*.epe)
if [ "${#files[@]}" -eq 0 ]; then
  echo "check-library: no patterns found in $DIR" >&2
  exit 1
fi

# ---- directive lint -----------------------------------------------------------
# A `//# min= max= step= default=` control directive parses in exactly two
# placements (web/src/lib/hints.ts, mirrored in tools/verify/review/engine.js):
#
#   export function sliderSpeed(v) { … }  //# min=0 max=5 default=2   (trailing)
#   //# min=0 max=5 default=2
#   export function sliderSpeed(v) { … }                             (line above)
#
# Anywhere else — most commonly the first line INSIDE the handler body, which
# reads perfectly naturally — the comment is inert: the control degrades to an
# unbounded 0..1 slider with no default, silently. Gitea #179 found ~13 library
# patterns shipping directives that did nothing. So: fail the sweep on one.
#
# Only lines that actually look like a directive are flagged (a `//#` followed
# by one of the four recognized keys and an `=`), so prose that merely mentions
# "//#" in a comment is left alone.
lint_out=""
for f in "$DIR"/*.js; do
  hits=$(awk '
    { line[FNR] = $0; last = FNR }
    END {
      ex = "export[ \t]+function[ \t]+[A-Za-z_$][A-Za-z0-9_$]*[ \t]*\\("
      for (i = 1; i <= last; i++) {
        if (line[i] !~ /\/\/#[^\n]*(min|max|step|default)[ \t]*=/) continue
        if (line[i] ~ ("^.*" ex "[^)]*\\).*//#")) continue                       # trailing
        if (line[i] ~ /^[ \t]*\/\/#/ && i < last && line[i+1] ~ ("^[ \t]*" ex)) continue  # line above
        sub(/^[ \t]+/, "", line[i])
        printf "  %s:%d: %s\n", FILENAME, i, line[i]
      }
    }' "$f")
  if [ -n "$hits" ]; then lint_out="$lint_out$hits"$'\n'; fi
done
if [ -n "$lint_out" ]; then
  printf 'check-library: detached //# control directive(s) — silently ignored by the\n' >&2
  printf '  parser. Move each onto the export line, or onto the line directly above it.\n' >&2
  printf '%s' "$lint_out" >&2
  exit 1
fi
echo "directive lint: no detached //# directives in $DIR"

cargo build --release -p luxel-cli
LUXEL="$ROOT/target/release/luxel"

total_fail=0
for grid in $GRIDS; do
  pass=0
  fail=0
  for f in "${files[@]}"; do
    if [ "$grid" = default ]; then
      out=$("$LUXEL" check "$f" 2>&1) && rc=0 || rc=$?
    else
      out=$("$LUXEL" check "$f" --grid "$grid" 2>&1) && rc=0 || rc=$?
    fi
    if [ "$rc" -eq 0 ]; then
      pass=$((pass + 1))
    else
      fail=$((fail + 1))
      total_fail=$((total_fail + 1))
      printf 'FAIL [%s] %s\n  %s\n' "$grid" "$f" "$out"
    fi
  done
  printf '%s grid: %d/%d ok\n' "$grid" "$pass" "$((pass + fail))"
done

if [ "$total_fail" -ne 0 ]; then
  echo "check-library: $total_fail failure(s) across $DIR" >&2
  exit 1
fi
echo "check-library: $DIR clean on all grids ($GRIDS)"
