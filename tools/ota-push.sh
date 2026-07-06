#!/usr/bin/env bash
# Push a firmware update over the air and verify the device comes back on
# the new slot. Usage:
#   tools/ota-push.sh <host> [image]
# Default image: the devshell Xtensa build (run build-esp32.sh first), or
# pass a nix-built result/luxel-fw-ota.bin explicitly.
set -euo pipefail

HOST="${1:?usage: ota-push.sh <host> [app-image.bin]}"
IMAGE="${2:-}"

cd "$(dirname "$0")/.."

if [ -z "$IMAGE" ]; then
  ELF=firmware/target/xtensa-esp32-none-elf/release/luxel-fw
  [ -f "$ELF" ] || { echo "no $ELF — run firmware/build-esp32.sh first (or pass an image)"; exit 1; }
  IMAGE=$(mktemp --suffix=.bin)
  trap 'rm -f "$IMAGE"' EXIT
  espflash save-image --chip esp32 "$ELF" "$IMAGE"
fi

before=$(curl -sf "http://$HOST/api/status" | tr ',' '\n' | grep '"slot"' || true)
echo "device: http://$HOST  $before"
echo "pushing $(stat -c%s "$IMAGE") bytes…"

resp=$(curl -sf --data-binary "@$IMAGE" -H 'Content-Type: application/octet-stream' \
  --max-time 300 "http://$HOST/api/ota")
echo "device: $resp"
case "$resp" in *'"ok":true'*) ;; *) echo "OTA rejected"; exit 1;; esac

echo "waiting for reboot…"
sleep 4
for i in $(seq 1 30); do
  if status=$(curl -sf --max-time 2 "http://$HOST/api/status"); then
    echo "back up: $status"
    exit 0
  fi
  sleep 2
done
echo "device did not come back within 60 s — check serial"
exit 1
