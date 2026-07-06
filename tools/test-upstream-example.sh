#!/usr/bin/env bash
# Discrimination test: build & flash the UNMODIFIED upstream esp-hal
# embassy_dhcp example (scans, connects, loops HTTP GETs) on the esp32 at
# the exact git rev our firmware pins. If IT crashes like Luxel does, the
# bug is upstream (esp-radio on classic ESP32) and we file a trivial-repro
# issue; if it survives minutes of traffic, the trigger is in our firmware
# and we bisect our components.
#
# Usage (devshell, device in bootloader mode):
#   LUXEL_SSID='net' LUXEL_PASS='secret' tools/test-upstream-example.sh
#
# NOTE: this flashes the example with espflash's default partition table,
# overwriting the OTA layout. Reflashing Luxel afterwards (build-esp32.sh
# flash) restores it — nothing is lost.
set -euo pipefail

REV=7c7f372
SRC=$(ls -d ~/.cargo/git/checkouts/esp-hal-*/"$REV" 2>/dev/null | head -1)
[ -n "$SRC" ] || { echo "esp-hal git checkout not found — build the firmware once first"; exit 1; }
DST=${TMPDIR:-/tmp}/esp-hal-upstream-test
rm -rf "$DST"
cp -r "$SRC" "$DST"
chmod -R u+w "$DST"

# resolve before any cd — $0 is relative to the caller's cwd
REPO=$(cd "$(dirname "$0")/.." && pwd)

cd "$DST/examples/wifi/embassy_dhcp"

# MODE=server replaces the client loop with a bare embassy-net TCP server
# (tools/upstream-server-test/main.rs) — still 100% upstream crates; this
# reproduces the serve-side TX burst that crashes Luxel on esp32.
if [ "${MODE:-client}" = "server" ]; then
  cp "$REPO/tools/upstream-server-test/main.rs" src/main.rs
  grep -q '^embedded-io-async' Cargo.toml || \
    sed -i '/^\[dependencies\]/a embedded-io-async = "0.7"' Cargo.toml
fi

export RUSTC="$XTENSA_RUST_HOME/bin/rustc"
export RUSTDOC="$XTENSA_RUST_HOME/bin/rustdoc"
SSID="${LUXEL_SSID:?set LUXEL_SSID}" PASSWORD="${LUXEL_PASS:?set LUXEL_PASS}" \
  exec "$XTENSA_RUST_HOME/bin/cargo" run --release --features esp32 --target xtensa-esp32-none-elf
