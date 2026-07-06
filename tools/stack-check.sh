#!/usr/bin/env bash
# Build the classic-ESP32 firmware with per-function stack sizes and fail if
# any frame exceeds a byte budget. Unlike clippy::large_stack_arrays (which
# only sees our own source), this inspects EVERY function in the linked
# image — deps and build-std core/alloc included — so it catches library
# frames like esp-storage's `FlashStorage::read` 4 KiB bounce buffer, the
# original OTA-crash culprit. Run in the nix devshell:
#   nix develop -c tools/stack-check.sh [budget_bytes]   (default 12288)
set -euo pipefail
TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$TOOLS_DIR/../firmware"
BOARD="${BOARD:-board-pixelblaze-v3}"
BUDGET="${1:-12288}"

TC="${XTENSA_RUST_HOME:-$HOME/.rustup/toolchains/esp}"
if [ ! -x "$TC/bin/cargo" ]; then
  echo "Xtensa toolchain not found ($TC) — enter the nix devshell." >&2
  exit 1
fi
export RUSTC="$TC/bin/rustc" RUSTDOC="$TC/bin/rustdoc"
# RUSTFLAGS overrides (does not merge with) the config's rustflags, so
# replicate the xtensa link flags here and append -Z emit-stack-sizes.
export RUSTFLAGS="-C link-arg=-Wl,-Tlinkall.x -C link-arg=-nostartfiles -Z emit-stack-sizes"
[ -f creds.env ] && . ./creds.env || true

echo "building with -Z emit-stack-sizes (board $BOARD)…"
"$TC/bin/cargo" build --release \
  --no-default-features --features "$BOARD" \
  --target xtensa-esp32-none-elf \
  -Zbuild-std=core,alloc

exec python3 "$TOOLS_DIR/stack-check.py" \
  target/xtensa-esp32-none-elf/release/luxel-fw "$BUDGET"
