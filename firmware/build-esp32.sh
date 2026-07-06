#!/usr/bin/env bash
# Build (and optionally flash) the classic-ESP32 (Xtensa) firmware.
# The Xtensa toolchain (Espressif's rustc fork + GNU linker) is provided by
# the nix devshell — just `nix develop` and run this script.
# Usage: [BOARD=board-pixelblaze-v3|board-athom-music] ./build-esp32.sh [flash]
set -euo pipefail
cd "$(dirname "$0")"

BOARD="${BOARD:-board-pixelblaze-v3}"

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
  # flash + monitor: tee the monitor session (symbolicated by espflash)
  # into serial.log so it's remotely readable
  "$TC/bin/cargo" run --release \
    --no-default-features --features "$BOARD" \
    --target xtensa-esp32-none-elf \
    -Zbuild-std=core,alloc 2>&1 | tee -a serial.log
else
  "$TC/bin/cargo" build --release \
    --no-default-features --features "$BOARD" \
    --target xtensa-esp32-none-elf \
    -Zbuild-std=core,alloc
fi
