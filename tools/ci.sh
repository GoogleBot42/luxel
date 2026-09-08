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
#   4a. devshell firmware build (CI_BOARD, default board-pixelblaze-v3) —
#       covers build-esp32.sh itself + the linked-feature markers
#   4b. tools/image-check.sh over the THREE release images the flake builds
#       (markers + the 1 MiB OTA-slot margin), byte-identical to release.yml
#   5. OPT-IN (CI_QEMU=1): tools/qemu/run-all.py, the emulator suite
#
# Step 5 is off by default on purpose (Gitea #273): it wants a from-source
# build of Espressif's QEMU fork, which is minutes on a cold runner, and five
# of its seven tests need the gitignored Athom flash dumps, which CI does not
# have — they SKIP there, so the hosted gate would buy one test for a QEMU
# build. Run it locally before merging anything under firmware/src/takeover.rs,
# ota.rs, flashmap.rs or the partition table:
#
#   CI_QEMU=1 CI_SKIP="web cargo library firmware" nix develop --command tools/ci.sh
#
# Env knobs:
#   CI_BOARD     board for the devshell build (default board-pixelblaze-v3)
#   CI_VARIANTS  flake firmware variants to image-check, space separated,
#                spelled as in release.yml's matrix (default: the three below;
#                empty string skips the whole image-check half)
#   CI_SKIP      space-separated step names to skip: web cargo library firmware
#   CI_QEMU      set to 1 to add the (opt-in) QEMU suite as a final step
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"

BOARD="${CI_BOARD:-board-pixelblaze-v3}"

# THREE release images, not one devshell build (Gitea #413 / #438). Until
# 2026-09-08 this step built board-pixelblaze-v3 in the devshell and ran the
# margin gate over THAT, and two release-gate breakages merged green on the
# same day because of it: board-c3-devkit stopped compiling at all
# (riscv32imc has no atomic read-modify-write, so an ungated fetch_add in
# shared.rs is a hard error there) and the C6 image release.yml actually
# ships fell under image-check's 3 % OTA-slot floor.
#
# Two things changed. The list covers the three axes the other five images
# share — Xtensa + -Zbuild-std (pixelblaze-v3, which the athom-music,
# esp32-generic, s3-devkit, s3-hub75 and seengreat-hub75 images differ from
# only in pin maps and features), the tightest image in the fleet
# (c6-devkit-hosted), and the only riscv32imc target (c3-devkit).
#
# And the margin is measured on the FLAKE image, not a devshell one. They
# are not the same artifact: a devshell build bakes the dev WiFi creds, and
# the gate has to weigh the bytes release.yml actually publishes. `nix build
# .#luxel-fw-<variant>` is byte-identical to the release asset, so this loop
# is release.yml's loop; keep the two in step.
#
# The bigger half of that gap is gone since Gitea #441: a devshell build used
# to additionally embed the absolute path of every dependency source file in
# its panic `Location`s, reading ~2.8 KB larger than the flake image AND by a
# different amount on every machine (this repo's own docs measured one commit
# at three sizes; the CI runner's /var/lib/gitea-runner/… path failed the 3 %
# floor the dev host passed). `--remap-path-prefix` now strips those
# everywhere, and the two builds agree to ~160 B. Do NOT take that as licence
# to gate the devshell image instead: what is left is the creds, whose length
# still moves the number, and CI's placeholder creds.env is not the release
# artifact's anyway.
VARIANTS="${CI_VARIANTS-pixelblaze-v3 c6-devkit-hosted c3-devkit}"
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
  step "firmware: $BOARD devshell build + image-check (ELF markers)"
  BOARD="$BOARD" firmware/build-esp32.sh
  done_step "firmware: $BOARD"

  # The release images. `nix build` is a pure, credless build in the nix
  # sandbox — the same derivation release.yml builds, so the bytes
  # image-check weighs here are the bytes that get published. EXPECT_FEATURES
  # is release.yml's own case, copied: the variant name is not always the
  # board (c6-devkit-hosted is board-c6-devkit + hosted-ui, s3-hub75 is
  # board-s3-devkit + hub75) and `hosted-ui` additionally asserts the
  # ABSENT markers, which is the half that catches a hosted image that
  # quietly kept its asset reader.
  mkdir -p "$ROOT/firmware/target"   # the out-links land here; may not exist yet
  for v in $VARIANTS; do
    step "release image: luxel-fw-$v + image-check"
    extras=""
    case "$v" in
      *hub75)   extras="hub75";;
      *-hosted) extras="hosted-ui";;
    esac
    case "$v" in
      s3-hub75)         extras="$extras board-s3-devkit";;
      c6-devkit-hosted) extras="$extras board-c6-devkit";;
      *)                extras="$extras board-$v";;
    esac
    out="$ROOT/firmware/target/ci-result-$v"
    nix build ".#luxel-fw-$v" --out-link "$out"
    EXPECT_FEATURES="$extras" tools/image-check.sh "$out/luxel-fw-ota.bin"
    done_step "release image: $v"
  done
fi

# --------------------------------------------------------------- qemu
# Opt-in: see the header. run-all.py builds .#luxel-fw-athom-music and
# .#qemu-espressif itself (nix-cached) and skips, rather than fails, the
# tests whose Athom flash dumps are absent.
if [ "${CI_QEMU:-0}" = 1 ]; then
  step "qemu: tools/qemu/run-all.py (opt-in)"
  python3 tools/qemu/run-all.py
  done_step qemu
else
  echo
  echo "== qemu: SKIPPED (opt-in — rerun with CI_QEMU=1)"
fi

echo
echo "=============================================================="
echo "== CI gate GREEN in $(( $(date +%s) - start ))s"
echo "=============================================================="
