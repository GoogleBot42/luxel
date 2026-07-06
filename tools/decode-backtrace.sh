#!/usr/bin/env bash
# Symbolicate ESP32 backtrace addresses from a raw serial log against the
# current ELF. Usage:
#   tools/decode-backtrace.sh [elf] < snippet-with-0x40xxxxxx-addresses
#   grep -o '0x40[0-9a-f]*' firmware/serial.log | tail -30 | tools/decode-backtrace.sh
set -euo pipefail
cd "$(dirname "$0")/.."

ELF="${1:-firmware/target/xtensa-esp32-none-elf/release/luxel-fw}"
A2L=$(command -v xtensa-esp32-elf-addr2line || command -v xtensa-esp-elf-addr2line)

grep -o '0x40[0-9a-fA-F]\{6\}' | while read -r addr; do
  line=$("$A2L" -e "$ELF" -f -C "$addr" | paste -sd' @ ')
  echo "$addr  $line"
done
