#!/usr/bin/env bash
# Build the firmware (any BOARD — chip/target/toolchain come from
# firmware/board-target.sh) with per-function stack sizes and fail if
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
# shellcheck source=../firmware/board-target.sh
. ./board-target.sh
board_target "$BOARD"
# Same knob as firmware/build-esp32.sh: extra non-board cargo features
# (space-separated), e.g. EXTRA_FEATURES=small-chip. The small-chip profile
# moves 8 KB of task arena into the heap, so its .stack floor must be
# checked separately from the default build's.
FEATURES="$BOARD${EXTRA_FEATURES:+ $EXTRA_FEATURES}"

CARGO=cargo
STD_FLAGS=()
# RUSTFLAGS overrides (does not merge with) the config's rustflags, so
# replicate the per-arch link flags here and append -Z emit-stack-sizes.
# -Z on a stable toolchain needs RUSTC_BOOTSTRAP (the Xtensa fork is
# nightly-based and doesn't).
if [ "$XTENSA" = 1 ]; then
  TC="${XTENSA_RUST_HOME:-$HOME/.rustup/toolchains/esp}"
  if [ ! -x "$TC/bin/cargo" ]; then
    echo "Xtensa toolchain not found ($TC) — enter the nix devshell." >&2
    exit 1
  fi
  export RUSTC="$TC/bin/rustc" RUSTDOC="$TC/bin/rustdoc"
  CARGO="$TC/bin/cargo"
  STD_FLAGS=(-Zbuild-std=core,alloc)
  export RUSTFLAGS="-C link-arg=-Wl,-Tlinkall.x -C link-arg=-nostartfiles -Z emit-stack-sizes"
else
  export RUSTC_BOOTSTRAP=1
  export RUSTFLAGS="-C link-arg=-Tlinkall.x -C force-frame-pointers -Z emit-stack-sizes"
fi
[ -f creds.env ] && . ./creds.env || true

echo "building with -Z emit-stack-sizes (features: $FEATURES, target $TARGET)…"
"$CARGO" build --release \
  --no-default-features --features "$FEATURES" \
  --target "$TARGET" \
  "${STD_FLAGS[@]}"

ELF=target/$TARGET/release/luxel-fw

# The leftover-DRAM main-task stack (.stack = whatever the statics don't
# claim; see main.rs's heap_allocator comment). v0.1.31 "estimated" it at
# ~27 KB while shipping 18 KB — 2 KB above the measured 15.6 KB overflow
# point — and every request-context flash read panicked the Athom
# (v0.1.33). 31 KB ran clean for weeks; alarm with margin below that.
STACK_FLOOR=$((24 * 1024))
STACK_SIZE=$(python3 - "$ELF" <<'EOF'
import struct, sys
d = open(sys.argv[1], 'rb').read()
shoff, shentsize, shnum, shstrndx = (
    struct.unpack_from('<I', d, 32)[0],
    *struct.unpack_from('<HHH', d, 46),
)
def sh(i):
    return struct.unpack_from('<10I', d, shoff + i * shentsize)
stroff = sh(shstrndx)[4]
for i in range(shnum):
    s = sh(i)
    name = d[stroff + s[0]:d.index(b'\0', stroff + s[0])]
    if name == b'.stack':
        print(s[5])
        break
else:
    print(0)
EOF
)
echo ".stack (main-task stack) = $STACK_SIZE bytes"
if [ "$STACK_SIZE" -lt "$STACK_FLOOR" ]; then
  echo "FAIL: main-task stack < $STACK_FLOOR bytes — statics grew;" >&2
  echo "shrink this chip's heap_allocator in main.rs (measure, don't estimate)" >&2
  exit 1
fi

exec python3 "$TOOLS_DIR/stack-check.py" "$ELF" "$BUDGET"
