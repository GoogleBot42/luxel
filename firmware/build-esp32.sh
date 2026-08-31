#!/usr/bin/env bash
# Build (and optionally flash) the firmware for any board. Despite the name
# this drives every target, not just the classic ESP32: the chip, rust
# target and toolchain all come from $BOARD via board-target.sh. Xtensa
# boards (esp32, esp32s3) need Espressif's rustc fork + GNU linker and
# -Zbuild-std; RISC-V boards (esp32c3, esp32c6) build with mainline Rust.
# Both toolchains come from the nix devshell — just `nix develop` and run
# this script.
#
# Usage: [BOARD=board-pixelblaze-v3|board-s3-devkit|…] ./build-esp32.sh [flash|image|log]
#   (none)  build only
#   flash   flash app + WEB ASSETS + monitor. The assets partition
#           (0x310000) gets the freshly packed playground too, so a serial
#           flash never leaves a stale web app (SKIP_ASSETS=1 to opt out;
#           EXTRA_FEATURES=hosted-ui skips it by construction).
#   image   write target/luxel-full.bin — a single full-flash image
#           (bootloader + partition table + app + assets) for
#           `espflash write-bin 0x0 target/luxel-full.bin`.
#   log     attach the serial monitor only.
set -euo pipefail
cd "$(dirname "$0")"

BOARD="${BOARD:-board-pixelblaze-v3}"
# shellcheck source=board-target.sh
. ./board-target.sh
board_target "$BOARD"

# The WLED→Luxel takeover self-install (src/takeover.rs) is always built
# in — a no-op on devices already running the Luxel partition layout. To
# produce the image WLED's /update page accepts, use espflash save-image
# (NOT the merged image): see docs/wled-migration.md.
FEATURES="$BOARD"
# Extra non-board cargo features, space-separated. The one in practical use
# is `small-chip` (RAM-constrained profile: web pool 2 + 88 KB heap + tuned
# WiFi buffer pools — docs/boards.md tiers). Example:
#   EXTRA_FEATURES=small-chip BOARD=board-athom-music ./build-esp32.sh
if [ -n "${EXTRA_FEATURES:-}" ]; then
  FEATURES="$FEATURES $EXTRA_FEATURES"
fi
# `hosted-ui` (Gitea #11): the image has no asset reader and no /api/assets,
# so there is nothing to pack and nothing to write at 0x310000 — the web app
# lives on the hosted playground instead. See docs/boards.md.
HOSTED_UI=0
case " $FEATURES " in *" hosted-ui "*) HOSTED_UI=1 ;; esac

# Dev WiFi creds: auto-source the git-ignored creds.env so every build —
# whoever runs it — bakes working credentials. An image without creds
# boots offline and is unreachable for OTA (locked out the device twice
# now: 2026-07-05 and 2026-07-06). tools/ota-push.sh refuses such images.
if [ -f creds.env ]; then
  # shellcheck source=/dev/null
  . ./creds.env
fi
if [ -z "${LUXEL_SSID:-}" ]; then
  echo "WARNING: LUXEL_SSID unset and no firmware/creds.env — this build will be OFFLINE-ONLY" >&2
fi

# `log`: attach exactly the way `flash --monitor` does (same espflash
# monitor engine, default reset handling, ELF symbolication so backtraces
# are readable), appending to firmware/serial.log for remote reading.
if [ "${1:-}" = "log" ]; then
  ELF=target/$TARGET/release/luxel-fw
  [ -f "$ELF" ] || { echo "no $ELF — build first" >&2; exit 1; }
  echo "logging to $(pwd)/serial.log (Ctrl-C to stop)"
  espflash monitor --chip "$CHIP" --elf "$ELF" 2>&1 | tee -a serial.log
fi

CMD=build
if [ "${1:-}" = "flash" ]; then CMD=run; fi
if [ "${1:-}" = "image" ]; then CMD=image; fi

# Pack the current playground into the LUXA archive that fills the assets
# partition (0x310000 in partitions.csv, served by src/assets.rs).
ASSETS_BIN="target/dist.luxa"
build_assets() {
  if [ "$HOSTED_UI" = 1 ]; then
    echo "hosted-ui build: no on-device web app, leaving the assets partition alone"
    return 1
  fi
  if [ "${SKIP_ASSETS:-}" = "1" ]; then
    echo "SKIP_ASSETS=1: leaving the assets partition alone"
    return 1
  fi
  echo "packing web assets…"
  # env -u: this script exports the XTENSA rustc; the web build's wasm step
  # needs the normal host toolchain
  (cd ../web \
    && env -u RUSTC -u RUSTDOC npm run build >/dev/null \
    && node tools/pack-assets.mjs "../firmware/$ASSETS_BIN") \
    || { echo "web asset build failed — flashing without assets" >&2; return 1; }
}

# Xtensa boards: the devshell exports XTENSA_RUST_HOME (nix-built Espressif
# rustc fork, nightly-based — which -Zbuild-std needs) and puts the xtensa
# GNU linker on PATH. Fallback: an espup install at ~/.rustup/toolchains/esp.
# RISC-V boards use the devshell's mainline Rust (targets are prebuilt, so
# no -Zbuild-std).
CARGO=cargo
STD_FLAGS=()
if [ "$XTENSA" = 1 ]; then
  TC="${XTENSA_RUST_HOME:-$HOME/.rustup/toolchains/esp}"
  if [ ! -x "$TC/bin/cargo" ]; then
    echo "Xtensa toolchain not found ($TC)." >&2
    echo "Enter the nix devshell (nix develop) — it provides the toolchain." >&2
    exit 1
  fi
  export RUSTC="$TC/bin/rustc"
  export RUSTDOC="$TC/bin/rustdoc"
  CARGO="$TC/bin/cargo"
  STD_FLAGS=(-Zbuild-std=core,alloc)
fi

echo "board: $BOARD (chip $CHIP, target $TARGET)"
if [ "$CMD" = "run" ]; then
  # write the fresh asset bundle first (same serial session, independent
  # partition), then flash the app + attach the monitor
  if build_assets; then
    echo "flashing assets partition (0x310000)…"
    espflash write-bin 0x310000 "$ASSETS_BIN"
  fi
  # flash + monitor: tee the monitor session (symbolicated by espflash)
  # into serial.log so it's remotely readable
  "$CARGO" run --release \
    --no-default-features --features "$FEATURES" \
    --target "$TARGET" \
    "${STD_FLAGS[@]}" 2>&1 | tee -a serial.log
elif [ "$CMD" = "image" ]; then
  "$CARGO" build --release \
    --no-default-features --features "$FEATURES" \
    --target "$TARGET" \
    "${STD_FLAGS[@]}"
  HAVE_ASSETS=1
  build_assets || HAVE_ASSETS=0
  if [ "$HAVE_ASSETS" = 0 ] && [ "$HOSTED_UI" != 1 ]; then
    echo "image needs the assets (unset SKIP_ASSETS)"; exit 1
  fi
  OUT=target/luxel-full.bin
  # merged image = bootloader + partition table + app, laid out from 0x0…
  espflash save-image --chip "$CHIP" --merge --partition-table partitions.csv \
    "target/$TARGET/release/luxel-fw" "$OUT"
  # …then the asset bundle is written INTO the image at its partition
  # offset (espflash pads the merged image to the full 4 MB with 0xFF).
  # A hosted-ui image skips this: the assets partition stays erased.
  if [ "$HAVE_ASSETS" = 1 ]; then
    dd if="$ASSETS_BIN" of="$OUT" bs=4096 seek=$((0x310000 / 4096)) conv=notrunc status=none
  fi
  echo "full-flash image: firmware/$OUT ($(du -h "$OUT" | cut -f1)) — flash with:"
  echo "  espflash write-bin 0x0 firmware/$OUT"
else
  "$CARGO" build --release \
    --no-default-features --features "$FEATURES" \
    --target "$TARGET" \
    "${STD_FLAGS[@]}"
fi

# Load-bearing features must actually be linked (the //SIZETEST guard —
# see tools/image-check.sh). Runs for every command that produced an ELF.
ELF=target/$TARGET/release/luxel-fw
[ -f "$ELF" ] && EXPECT_FEATURES="$FEATURES" ../tools/image-check.sh "$ELF"
