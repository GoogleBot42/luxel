#!/usr/bin/env bash
# The CI gate — the exact sequence .gitea/workflows/ci.yml runs on every
# push to master and every PR. Run it locally to see what CI will see:
#
#   nix develop --command tools/ci.sh
#
# It MUST run inside the nix devshell: cargo, node and the Xtensa toolchain
# exist only there (a bare shell has none of them). Enter the devshell from
# the repo ROOT — the shellHook materializes firmware/vendor/esp-hub75
# relative to the shell's cwd, so entering it from inside firmware/ breaks
# the firmware build (.claude/skills/worktree-setup).
#
# Gates, in order (the order matters — see the comments):
#   1. web build   (wasm + gallery + svelte-check + vite build) + web tests
#   2. cargo test --workspace
#   3. tools/check-library.sh          (the library sweep, five rigs)
#   4. firmware build for board-pixelblaze-v3 + tools/image-check.sh
#      (linked-feature markers on the ELF, OTA-slot margin on the app image)
#
# Env knobs:
#   CI_BOARD    firmware board to build (default board-pixelblaze-v3)
#   CI_SKIP     space-separated step names to skip: web cargo library firmware
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"

BOARD="${CI_BOARD:-board-pixelblaze-v3}"
SKIP=" ${CI_SKIP:-} "

start=$(date +%s)
step_start=0
step() {
  step_start=$(date +%s)
  echo
  echo "=============================================================="
  echo "== $1"
  echo "=============================================================="
}
done_step() { echo "-- $1: $(( $(date +%s) - step_start ))s"; }
skipped() { case "$SKIP" in *" $1 "*) return 0 ;; *) return 1 ;; esac; }

command -v cargo >/dev/null || { echo "no cargo on PATH — run me inside \`nix develop\`" >&2; exit 1; }
command -v node  >/dev/null || { echo "no node on PATH — run me inside \`nix develop\`" >&2; exit 1; }

# ---------------------------------------------------------------- web
# Must run BEFORE cargo test: luxel-cli's heapstat test reads
# web/public/gallery.json, which `npm run build` (gen-gallery.mjs) writes.
# web/public is gitignored build output — `npm run wasm`'s cp fails hard if
# the directory doesn't exist yet, which is every fresh clone.
if skipped web; then echo "== web: SKIPPED"; else
  step "web: npm ci && npm run build && npm test"
  mkdir -p "$ROOT/web/public"
  cd "$ROOT/web"
  npm ci
  npm run build
  npm test
  cd "$ROOT"
  done_step web
fi

# -------------------------------------------------------------- cargo
if skipped cargo; then echo "== cargo: SKIPPED"; else
  step "cargo test --workspace"
  cargo test --workspace
  done_step cargo
fi

# ------------------------------------------------------------ library
if skipped library; then echo "== library: SKIPPED"; else
  step "tools/check-library.sh (the library sweep)"
  tools/check-library.sh
  done_step library
fi

# ----------------------------------------------------------- firmware
# firmware/build-esp32.sh with no argument builds only (no flash, no
# monitor, no device) and ends by running tools/image-check.sh over the ELF
# for the linked-feature markers. It sources the gitignored
# firmware/creds.env if present; CI writes a placeholder there, and a build
# without one only warns (the image is OFFLINE-ONLY, which is fine — CI
# never publishes an image).
if skipped firmware; then echo "== firmware: SKIPPED"; else
  step "firmware: $BOARD build + image-check"
  BOARD="$BOARD" firmware/build-esp32.sh
  # The ELF check above covers markers only. The 1 MiB OTA-slot margin gate
  # (Gitea #160, CLAUDE.md's tripwire) needs an actual app image, so make
  # one the way the flake does and run image-check over that too.
  . firmware/board-target.sh
  board_target "$BOARD"
  OTA="$ROOT/firmware/target/ci-ota.bin"
  espflash save-image --chip "$CHIP" \
    "$ROOT/firmware/target/$TARGET/release/luxel-fw" "$OTA"
  EXPECT_FEATURES="$BOARD" tools/image-check.sh "$OTA"
  done_step firmware
fi

echo
echo "=============================================================="
echo "== CI gate GREEN in $(( $(date +%s) - start ))s"
echo "=============================================================="
