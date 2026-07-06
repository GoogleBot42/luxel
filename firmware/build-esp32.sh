#!/usr/bin/env bash
# Build (and optionally flash) the classic-ESP32 (Xtensa) firmware.
# The Xtensa toolchain (Espressif's rustc fork + GNU linker) is provided by
# the nix devshell — just `nix develop` and run this script.
# Usage: [BOARD=board-pixelblaze-v3|board-athom-music] ./build-esp32.sh [flash]
set -euo pipefail
cd "$(dirname "$0")"

BOARD="${BOARD:-board-pixelblaze-v3}"

# `log`: attach to serial (no reset games — works with a bare RX/TX adapter)
# with symbolication from the current ELF so panics decode, appending
# everything to firmware/serial.log where it can be tailed/read remotely.
if [ "${1:-}" = "log" ]; then
  ELF=target/xtensa-esp32-none-elf/release/luxel-fw
  [ -f "$ELF" ] || { echo "no $ELF — build first" >&2; exit 1; }
  echo "logging to $(pwd)/serial.log (Ctrl-C to stop)"
  exec espflash monitor --non-interactive \
    --before no-reset-no-sync --after no-reset \
    --elf "$ELF" 2>&1 | tee -a serial.log
fi

CMD=build
if [ "${1:-}" = "flash" ]; then CMD=run; fi

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
"$TC/bin/cargo" "$CMD" --release \
  --no-default-features --features "$BOARD" \
  --target xtensa-esp32-none-elf \
  -Zbuild-std=core,alloc
