#!/usr/bin/env bash
# Build (and optionally flash) the classic-ESP32 (Xtensa) firmware.
# The Xtensa toolchain (Espressif's rustc fork + GNU linker) is provided by
# the nix devshell — just `nix develop` and run this script.
# Usage: [BOARD=board-pixelblaze-v3|board-athom-music] ./build-esp32.sh [flash]
set -euo pipefail
cd "$(dirname "$0")"

BOARD="${BOARD:-board-pixelblaze-v3}"

# `log`: dumb serial capture — no chip probing, no reset games, just bytes.
# Appends to firmware/serial.log (remotely tail-able); panic backtrace
# addresses decode with tools/decode-backtrace.sh.
if [ "${1:-}" = "log" ]; then
  PORT="${PORT:-/dev/ttyUSB0}"
  echo "logging $PORT @115200 to $(pwd)/serial.log (Ctrl-C to stop)"
  stty -F "$PORT" 115200 raw -echo -echoe -echok
  exec cat "$PORT" | tee -a serial.log
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
