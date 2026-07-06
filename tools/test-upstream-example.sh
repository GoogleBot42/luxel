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

cd "$DST/examples/wifi/embassy_dhcp"
export RUSTC="$XTENSA_RUST_HOME/bin/rustc"
export RUSTDOC="$XTENSA_RUST_HOME/bin/rustdoc"
SSID="${LUXEL_SSID:?set LUXEL_SSID}" PASSWORD="${LUXEL_PASS:?set LUXEL_PASS}" \
  exec "$XTENSA_RUST_HOME/bin/cargo" run --release --features esp32 --target xtensa-esp32-none-elf
