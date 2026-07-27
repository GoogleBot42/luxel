#!/usr/bin/env bash
# Build (and optionally flash) the classic-ESP32 (Xtensa) firmware.
# The Xtensa toolchain (Espressif's rustc fork + GNU linker) is provided by
# the nix devshell — just `nix develop` and run this script.
#
# Usage: [BOARD=board-pixelblaze-v3|board-athom-music] ./build-esp32.sh [flash|image|log]
#   (none)  build only
#   flash   flash app + WEB ASSETS + monitor. The assets partition
#           (0x310000) gets the freshly packed playground too, so a serial
#           flash never leaves a stale web app (SKIP_ASSETS=1 to opt out).
#   image   write target/luxel-full.bin — a single full-flash image
#           (bootloader + partition table + app + assets) for
#           `espflash write-bin 0x0 target/luxel-full.bin`.
#   log     attach the serial monitor only.
set -euo pipefail
cd "$(dirname "$0")"

BOARD="${BOARD:-board-pixelblaze-v3}"

# The WLED→Luxel takeover self-install (src/takeover.rs) is always built
# in — a no-op on devices already running the Luxel partition layout. To
# produce the image WLED's /update page accepts, use espflash save-image
# (NOT the merged image): see docs/wled-migration.md.
FEATURES="$BOARD"

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
  ELF=target/xtensa-esp32-none-elf/release/luxel-fw
  [ -f "$ELF" ] || { echo "no $ELF — build first" >&2; exit 1; }
  echo "logging to $(pwd)/serial.log (Ctrl-C to stop)"
  espflash monitor --chip esp32 --elf "$ELF" 2>&1 | tee -a serial.log
fi

CMD=build
if [ "${1:-}" = "flash" ]; then CMD=run; fi
if [ "${1:-}" = "image" ]; then CMD=image; fi

# Pack the current playground into the LUXA archive that fills the assets
# partition (0x310000 in partitions.csv, served by src/assets.rs).
ASSETS_BIN="target/dist.luxa"
build_assets() {
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

# The devshell exports XTENSA_RUST_HOME (nix-built Espressif rustc fork,
# nightly-based — which -Zbuild-std needs) and puts the xtensa GNU linker on
# PATH. Fallback: an espup install at ~/.rustup/toolchains/esp.
TC="${XTENSA_RUST_HOME:-$HOME/.rustup/toolchains/esp}"
if [ ! -x "$TC/bin/cargo" ]; then
  echo "Xtensa toolchain not found ($TC)." >&2
  echo "Enter the nix devshell (nix develop) — it provides the toolchain." >&2
  exit 1
fi
export RUSTC="$TC/bin/rustc"
export RUSTDOC="$TC/bin/rustdoc"

echo "board: $BOARD"
if [ "$CMD" = "run" ]; then
  # write the fresh asset bundle first (same serial session, independent
  # partition), then flash the app + attach the monitor
  if build_assets; then
    echo "flashing assets partition (0x310000)…"
    espflash write-bin 0x310000 "$ASSETS_BIN"
  fi
  # flash + monitor: tee the monitor session (symbolicated by espflash)
  # into serial.log so it's remotely readable
  "$TC/bin/cargo" run --release \
    --no-default-features --features "$FEATURES" \
    --target xtensa-esp32-none-elf \
    -Zbuild-std=core,alloc 2>&1 | tee -a serial.log
elif [ "$CMD" = "image" ]; then
  "$TC/bin/cargo" build --release \
    --no-default-features --features "$FEATURES" \
    --target xtensa-esp32-none-elf \
    -Zbuild-std=core,alloc
  build_assets || { echo "image needs the assets (unset SKIP_ASSETS)"; exit 1; }
  OUT=target/luxel-full.bin
  # merged image = bootloader + partition table + app, laid out from 0x0…
  espflash save-image --chip esp32 --merge --partition-table partitions.csv \
    target/xtensa-esp32-none-elf/release/luxel-fw "$OUT"
  # …then the asset bundle is written INTO the image at its partition
  # offset (espflash pads the merged image to the full 4 MB with 0xFF)
  dd if="$ASSETS_BIN" of="$OUT" bs=4096 seek=$((0x310000 / 4096)) conv=notrunc status=none
  echo "full-flash image: firmware/$OUT ($(du -h "$OUT" | cut -f1)) — flash with:"
  echo "  espflash write-bin 0x0 firmware/$OUT"
else
  "$TC/bin/cargo" build --release \
    --no-default-features --features "$FEATURES" \
    --target xtensa-esp32-none-elf \
    -Zbuild-std=core,alloc
fi
