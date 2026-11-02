#!/usr/bin/env bash
# Per-batch pattern verification. Run INSIDE the devshell:
#   nix develop -c bash tools/verify/verify-pattern.sh <outdir> library/a.js library/b.js …
# 1. tools/check-library.sh over a temp dir holding ONLY your files: the three
#    source lints (detached //# directive, 2-arg arrayReplace, unitless 0..1
#    slider) + luxel check on the five CI rigs.
# 2. luxel check --grid 64x64 (the array budget on a real panel).
# 3. contact sheets at 32x32 (<outdir>/<stem>.png, 8 frames over 8 s) and 64x64
#    (<outdir>/64/<stem>.png) — LOOK at them — plus host ms/frame in the .json.
# Exits non-zero if 1 or 2 failed.
set -u
W="$(cd "$(dirname "$0")/../.." && pwd)"
LUXEL=$W/target/release/luxel
out=$1; shift
mkdir -p "$out" "$out/lint"
rm -f "$out"/lint/*.js
for f in "$@"; do cp "$f" "$out/lint/"; done
fail=0
(cd "$W" && bash tools/check-library.sh "$out/lint") || fail=1
for f in "$@"; do
  res=$("$LUXEL" check "$f" --grid 64x64 2>&1)
  case "$res" in *'"stage":"ok"'*) ;; *) echo "FAIL 64x64 $f: $res" | head -c 600; echo; fail=1;; esac
done
node $W/tools/verify/sheet.mjs "$out" --grid 32x32 "$@"
node $W/tools/verify/sheet.mjs "$out/64" --grid 64x64 --frames 4 --scale 2 "$@" | sed 's/^/64x64: /'
[ $fail = 0 ] && echo "VERIFY OK" || echo "VERIFY FAILED (see above)"
exit $fail
