#!/usr/bin/env bash
# Push a firmware update over the air and verify the device comes back on
# the new slot. Usage:
#   [BOARD=board-...] tools/ota-push.sh <host> [image]
# Default image: the devshell build for $BOARD (run build-esp32.sh first),
# or pass a nix-built result/luxel-fw-ota.bin explicitly.
#
# $BOARD picks the ELF path and the espflash --chip, through the same
# board-target.sh map build-esp32.sh uses — it used to be hardcoded to the
# classic ESP32, so `BOARD=board-seengreat-hub75 tools/deploy.sh <ip>`
# (deploy.sh calls this) died on a missing xtensa-esp32-none-elf ELF while
# a perfectly good S3 build sat in the tree (2026-09-06).
#
# The image is checked against $BOARD before it goes out: it must contain
# that board's `board::NAME` string (Gitea #389). An image passed as $2 is
# only checked when $BOARD is set explicitly. SKIP_BOARD_CHECK=1 opts out.
set -euo pipefail

HOST="${1:?usage: ota-push.sh <host> [app-image.bin]}"
IMAGE="${2:-}"

cd "$(dirname "$0")/.."

# Remember whether the caller picked the board: an image passed explicitly
# on the command line (e.g. a nix-built result/luxel-fw-ota.bin) carries its
# own board, so it must not be checked against the script's default.
BOARD_EXPLICIT="${BOARD:+1}"
IMAGE_GIVEN="${IMAGE:+1}"
BOARD="${BOARD:-board-pixelblaze-v3}"
# shellcheck source=../firmware/board-target.sh
. firmware/board-target.sh
board_target "$BOARD"

if [ -z "$IMAGE" ]; then
  ELF=firmware/target/$TARGET/release/luxel-fw
  [ -f "$ELF" ] || { echo "no $ELF — run BOARD=$BOARD firmware/build-esp32.sh first (or pass an image)"; exit 1; }
  IMAGE=$(mktemp --suffix=.bin)
  trap 'rm -f "$IMAGE"' EXIT
  espflash save-image --chip "$CHIP" "$ELF" "$IMAGE"
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

# Guard (Gitea #389): the image must actually BE a build of $BOARD. The
# three classic-ESP32 boards share one ELF path, so a stale build of
# another board pushes cleanly, boots fine, and differs only in
# RESERVED_PINS / pin defaults — a pb-v3 image on the Athom reserved
# GPIO18, the pin the strip is wired to, and POST /api/datapin 18 then
# refused it. Every image bakes board::NAME in as a plain string
# (board-target.sh's board_name), so the mismatch is catchable here, the
# last point where it is still cheap. SKIP_BOARD_CHECK=1 opts out.
if [ "${SKIP_BOARD_CHECK:-0}" != 1 ] && { [ -z "$IMAGE_GIVEN" ] || [ -n "$BOARD_EXPLICIT" ]; }; then
  board_name "$BOARD"
  if ! grep -aqF "$BOARD_NAME" "$IMAGE"; then
    echo "REFUSING to push: image is not a $BOARD build" >&2
    echo "  (no \"$BOARD_NAME\" string in it)" >&2
    for b in $BOARD_LIST; do
      board_name "$b" || continue
      if grep -aqF "$BOARD_NAME" "$IMAGE"; then
        echo "  the image looks like a $b build (\"$BOARD_NAME\")" >&2
      fi
    done
    echo "Rebuild first — the board goes in \$BOARD, the positional is the action:" >&2
    echo "  (cd firmware && BOARD=$BOARD ./build-esp32.sh)" >&2
    exit 1
  fi
fi

before=$(curl -sf "http://$HOST/api/status" | tr ',' '\n' | grep '"slot"' || true)
echo "device: http://$HOST  $before"
echo "pushing $(stat -c%s "$IMAGE") bytes…"

# NB: keep the `|| rc=$?` — a bare `curl -sf` under `set -e` ends the script
# here on a non-2xx, silently, with the output stopping after "pushing N bytes…".
rc=0
resp=$(curl -s --show-error --fail-with-body --data-binary "@$IMAGE" \
  -H 'Content-Type: application/octet-stream' \
  --max-time 300 "http://$HOST/api/ota") || rc=$?
if [ "$rc" -ne 0 ]; then
  echo "OTA PUSH FAILED: curl exit $rc" >&2
  echo "device said: ${resp:-<nothing>}" >&2
  exit "$rc"
fi
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
