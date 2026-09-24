#!/usr/bin/env bash
# Build the playground engine, web/public/luxel.wasm — what `npm run wasm`
# runs, and therefore what `npm run build`, tools/ci.sh and
# firmware/build-esp32.sh's asset half all go through.
#
# Two things here that a plain `cargo build --release` does not do, both
# because this artifact ships inside the device's `.luxa` bundle, which CI
# gates at the 983,040 B assets partition (Gitea #683):
#
#   * `--profile wasm-release` (Cargo.toml) — opt-level "s", fat LTO,
#     codegen-units 1, panic=abort, strip: 759 kB raw → 541 kB.
#   * `wasm-opt -Oz` (binaryen, in the devshell) — 541 kB → 468 kB raw.
#     Worth ~2 kB gzipped on top of the profile; most of what it buys is the
#     RAW module the browser has to parse. Skipped with a warning if binaryen
#     is not on PATH, so a bare checkout still builds; the devshell always
#     has it, so CI is deterministic.
#
# Together: 254.0 kB → 193.1 kB gzip -9, or 242.1 kB → 183.7 kB as
# pack-assets.mjs zopfli-encodes it into the bundle.
#
# The size/speed table behind those choices is in web/tools/wasm-bench.mjs's
# header and the Gitea #683 PR; `opt-level = "z"` was measured 2.6x SLOWER
# and rejected. Re-run the bench after touching either.
set -euo pipefail

web="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo="$(dirname "$web")"
out="$web/public/luxel.wasm"

cd "$repo"
cargo build --profile wasm-release --target wasm32-unknown-unknown -p luxel-wasm
mkdir -p "$web/public"
cp "$repo/target/wasm32-unknown-unknown/wasm-release/luxel_wasm.wasm" "$out"

if command -v wasm-opt >/dev/null 2>&1; then
  # rustc's wasm32-unknown-unknown enables these by default, so binaryen has
  # to be told about them or it refuses the module as invalid.
  wasm-opt \
    --enable-bulk-memory --enable-bulk-memory-opt --enable-sign-ext \
    --enable-nontrapping-float-to-int --enable-mutable-globals \
    --enable-multivalue --enable-reference-types \
    -Oz "$out" -o "$out.opt"
  mv "$out.opt" "$out"
else
  echo "build-wasm: wasm-opt not on PATH — shipping the unoptimised module" \
       "(~68 kB larger raw). Use the devshell." >&2
fi

printf 'luxel.wasm: %s bytes\n' "$(stat -c%s "$out")"
