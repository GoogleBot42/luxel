#!/usr/bin/env bash
# Build (and optionally flash) the classic-ESP32 (Xtensa) firmware.
# Xtensa needs Espressif's rustc fork:
#   espup install --targets esp32          # one-time, from the devshell
#   ./patch-esp-toolchain.sh               # one-time, NixOS only
# Usage: [BOARD=board-pixelblaze-v3|board-athom-music] ./build-esp32.sh [flash]
set -euo pipefail
cd "$(dirname "$0")"

[ -f "$HOME/export-esp.sh" ] && . "$HOME/export-esp.sh"

BOARD="${BOARD:-board-pixelblaze-v3}"
CMD=build
if [ "${1:-}" = "flash" ]; then CMD=run; fi

# Use espup's Xtensa toolchain directly (its cargo is nightly, which
# build-std needs). NixOS note: espup's prebuilt binaries must be patchelf'd
# once after `espup install` — see docs/firmware.md.
TC="$HOME/.rustup/toolchains/esp"
export RUSTC="$TC/bin/rustc"
export RUSTDOC="$TC/bin/rustdoc"

echo "board: $BOARD"
"$TC/bin/cargo" "$CMD" --release \
  --no-default-features --features "$BOARD" \
  --target xtensa-esp32-none-elf \
  -Zbuild-std=core,alloc
