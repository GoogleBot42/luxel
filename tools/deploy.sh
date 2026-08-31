#!/usr/bin/env bash
# One-shot deploy to a device: firmware OTA + web asset bundle.
#
#   tools/deploy.sh <device-ip> [--fw-only | --assets-only]
#
# Builds everything from the working tree: wasm → playground → LUXA asset
# archive, and the ESP32 firmware (creds via firmware/creds.env, see
# build-esp32.sh). Firmware goes first (device reboots, ota-push.sh waits
# for it to come back); assets stream after via POST /api/assets (hot
# reload, no reboot). Run from the repo root inside `nix develop`.
#
# NOTE a serial `espflash flash` writes ONLY the app image — the assets
# partition (0x310000) keeps whatever it had. After any serial recovery,
# run `tools/deploy.sh <ip> --assets-only` to bring the web app current.
set -euo pipefail
cd "$(dirname "$0")/.."

IP="${1:?usage: tools/deploy.sh <device-ip> [--fw-only|--assets-only]}"
MODE="${2:-}"
BOARD="${BOARD:-board-pixelblaze-v3}"
LUXA="$(mktemp -t luxel-assets-XXXX.luxa)"
trap 'rm -f "$LUXA"' EXIT

if [[ "$MODE" != "--assets-only" ]]; then
    echo "== firmware: build ($BOARD) + OTA =="
    # BOARD is read from the environment by build-esp32.sh — it takes a
    # COMMAND as $1 (flash/image/log), so passing the board there silently
    # built the script's own default board instead.
    (cd firmware && BOARD="$BOARD" ./build-esp32.sh)
    tools/ota-push.sh "$IP"
fi

if [[ "$MODE" != "--fw-only" ]]; then
    echo "== assets: build + pack + push =="
    (cd web && npm run build >/dev/null && node tools/pack-assets.mjs "$LUXA")
    resp=$(curl -sfm 180 -X POST --data-binary "@$LUXA" "http://$IP/api/assets")
    echo "$resp"
    # /api/assets answers 200 + {"ok":false,…} on refusal (it mirrors the
    # rest of the API), so -f alone doesn't catch a hosted-ui device.
    case "$resp" in
      *'"ok":true'*) ;;
      *) echo "assets push rejected. A hosted-ui image (docs/boards.md) has no" >&2
         echo "on-device web app by design — use --fw-only against it." >&2
         exit 1 ;;
    esac
fi

echo "== deployed =="
curl -sfm 10 "http://$IP/api/status"
echo
