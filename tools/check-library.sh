#!/usr/bin/env bash
# Sweep `luxel check` over every pattern in library/ — the standing acceptance
# gate for engine / compiler / bytecode changes. Per pattern `check` does a
# compile, an LXBC serialize→deserialize→re-serialize round-trip (byte-identical
# required, plus the decoded program must render identically to the fresh
# compile — that is the device's execution path) and a 3-frame smoke run.
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

cargo build --release -p luxel-cli
LUXEL="$ROOT/target/release/luxel"

shopt -s nullglob
files=("$DIR"/*.js "$DIR"/*.epe)
if [ "${#files[@]}" -eq 0 ]; then
  echo "check-library: no patterns found in $DIR" >&2
  exit 1
fi

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
