#!/usr/bin/env bash
# Sweep `luxel check` over every pattern in library/ — the standing acceptance
# gate for engine / compiler / bytecode changes. Per pattern `check` does a
# compile, an LXBC serialize→deserialize→re-serialize round-trip (byte-identical
# required, plus the decoded program must render identically to the fresh
# compile — that is the device's execution path) and a 3-frame smoke run.
#
# Before the engine sweep it also runs two source lints:
#   * `//#` control directives — one that is not adjacent to an export is
#     silently ignored by the parser, so a pattern can ship a directive that
#     does nothing at all (Gitea #179). See "directive lint" below.
#   * two-argument `arrayReplace(a, v)` — the splat builtin misread as a fill,
#     which quietly freezes a buffer-based pattern. See "arrayReplace fill
#     lint" below (Gitea #225).
#
# Every pattern runs on FIVE rigs: two grids (`check`'s default 10x10 and an
# explicit 16x16) and three MAPLESS STRIPS (60, 300, 512 px). Rig shape is
# load-bearing — patterns that hardcode a rig width or do pixelCount arithmetic
# go out of bounds on one shape and not another.
#
# The strips are not redundant with the grids: with a map installed the engine
# picks `render2D`, so a 2D pattern's own 1D fallback is code the grid rigs
# NEVER execute. Gitea #193 was exactly that — a fallback that divided its row
# index by the matrix WIDTH, handing render2D a y >= 1 on any pixel count that
# isn't a perfect square. Clean on both grids, out of bounds on the 60 px strip
# the Athom actually runs, and it took a 45-minute hardware soak to notice.
#
# Run in the nix devshell:
#   nix develop -c tools/check-library.sh [dir]        (default dir: library)
#   GRIDS="default 16x16 8x32" STRIPS="60 1000" nix develop -c tools/check-library.sh
#   STRIPS= nix develop -c tools/check-library.sh      (grids only)
#
# Prints a per-rig pass count, the failing file + engine stage/error for each
# failure, and exits non-zero if anything failed. Baseline: 297/297 on every rig.
# (corpus/ has its own richer harness — tools/corpus/report.mjs.)
set -euo pipefail
TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$TOOLS_DIR/.." && pwd)"
cd "$ROOT"

DIR="${1:-library}"
# Space-separated rig list; "default" means no --grid flag (luxel check's own
# 10x10), anything else is passed through as --grid WxH.
GRIDS="${GRIDS:-default 16x16}"
# Space-separated mapless-strip pixel counts, each passed as --strip N. Kept
# clear of the PB-compat 10,240-element array budget (a few patterns legitimately
# exceed it at 2048 px, which is a budget rejection, not a bug).
STRIPS="${STRIPS-60 300 512}"

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

# ---- arrayReplace fill lint ---------------------------------------------------
# `arrayReplace(a, v1, v2, …)` SPLATS its value list into the array starting at
# index 0 — one value writes one slot. It is NOT a fill, so `arrayReplace(a, 0)`
# zeroes `a[0]` and leaves the rest of the buffer alone. The library read it as
# "set every element" for months (the playground docs said so too, until the
# 2026-08-31 review pass): 13 patterns were fixed in review pass 2 and the last
# 15 call sites under Gitea #225. The symptoms are quiet — an accumulation
# buffer that never clears saturates into a static wash, a "reset" resets
# nothing — so nothing fails, it just stops animating.
#
# So: fail the sweep on any TWO-ARGUMENT arrayReplace call. Zero a buffer with
# `feedback(a, 0)`, fill it with a constant via `arrayMutate(a, (v) => c)`, set
# one element with `arrayReplaceAt(a, i, v)`. A deliberate single-slot splat
# opts out with a `// arrayReplace-2arg-ok` comment on the same line.
#
# Occurrences inside a `//` comment are ignored — the fixed sites cite the old
# idiom in their explanatory comments — as are `arrayReplaceAt` and any longer
# identifier ending in "arrayReplace".
lint_out=""
for f in "$DIR"/*.js; do
  hits=$(awk '
    function twoArgReplace(src,   s, base, open, pre, i, ch, depth, commas, n) {
      s = src; base = 0; n = length(src)
      while (match(s, /arrayReplace[ \t]*\(/)) {
        open = base + RSTART + RLENGTH - 1                # index of "(" in src
        pre = (base + RSTART - 1 > 0) ? substr(src, base + RSTART - 1, 1) : ""
        if (pre !~ /[A-Za-z0-9_$.]/) {
          depth = 0; commas = 0
          for (i = open + 1; i <= n; i++) {
            ch = substr(src, i, 1)
            if (ch == "(") depth++
            else if (ch == ")") { if (depth == 0) break; depth-- }
            else if (ch == "," && depth == 0) commas++
          }
          if (i <= n && commas == 1) return 1             # balanced call, 2 args
        }
        base = base + RSTART + RLENGTH - 1
        s = substr(s, RSTART + RLENGTH)
      }
      return 0
    }
    {
      full = $0
      if (full ~ /arrayReplace-2arg-ok/) next
      code = $0
      ci = index(code, "//")
      if (ci > 0) code = substr(code, 1, ci - 1)
      if (twoArgReplace(code)) {
        sub(/^[ \t]+/, "", full)
        printf "  %s:%d: %s\n", FILENAME, FNR, full
      }
    }' "$f")
  if [ -n "$hits" ]; then lint_out="$lint_out$hits"$'\n'; fi
done
if [ -n "$lint_out" ]; then
  printf 'check-library: two-argument arrayReplace(a, v) — that is a SPLAT at index 0,\n' >&2
  printf '  not a fill: it writes a[0] and leaves the rest of the buffer untouched.\n' >&2
  printf '  Zero a buffer with feedback(a, 0), fill it with arrayMutate(a, (v) => c),\n' >&2
  printf '  set one element with arrayReplaceAt(a, i, v). A deliberate single-slot\n' >&2
  printf '  write opts out with a trailing // arrayReplace-2arg-ok comment. (#225)\n' >&2
  printf '%s' "$lint_out" >&2
  exit 1
fi
echo "arrayReplace lint: no two-argument arrayReplace() calls in $DIR"

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

for n in $STRIPS; do
  pass=0
  fail=0
  for f in "${files[@]}"; do
    out=$("$LUXEL" check "$f" --strip "$n" 2>&1) && rc=0 || rc=$?
    if [ "$rc" -eq 0 ]; then
      pass=$((pass + 1))
    else
      fail=$((fail + 1))
      total_fail=$((total_fail + 1))
      printf 'FAIL [strip %s] %s\n  %s\n' "$n" "$f" "$out"
    fi
  done
  printf '%s px strip: %d/%d ok\n' "$n" "$pass" "$((pass + fail))"
done

if [ "$total_fail" -ne 0 ]; then
  echo "check-library: $total_fail failure(s) across $DIR" >&2
  exit 1
fi
echo "check-library: $DIR clean on all rigs (grids: $GRIDS; strips: ${STRIPS:-none})"
