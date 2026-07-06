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

# Guard (hard lesson, twice): an image without baked WiFi creds boots
# offline and is UNREACHABLE for the next OTA — a remote lockout. The SSID
# is embedded as a plain string, so its absence is detectable. The SSID to
# expect comes from the env / firmware/creds.env.
if [ -z "${LUXEL_SSID:-}" ] && [ -f firmware/creds.env ]; then
  # shellcheck source=/dev/null
  . firmware/creds.env
fi
if [ -n "${LUXEL_SSID:-}" ]; then
  if ! grep -aqF "$LUXEL_SSID" "$IMAGE"; then
    echo "REFUSING to push: image does not contain the WiFi SSID — it would boot offline" >&2
    echo "(rebuild via firmware/build-esp32.sh, which sources firmware/creds.env)" >&2
    exit 1
  fi
else
  echo "REFUSING to push: LUXEL_SSID unknown (no env, no firmware/creds.env) — cannot verify the image has creds" >&2
  exit 1
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
