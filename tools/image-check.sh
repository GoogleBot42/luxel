#!/usr/bin/env bash
# Assert that load-bearing firmware features are actually LINKED into a
# built ELF or app image.
#
# Guards against the //SIZETEST regression class: commenting out a single
# call site still builds green while dead-code elimination silently strips
# the whole feature — the WLED takeover shipped dead this way from v0.1.31
# through v0.1.38 (UPDATES.md 2026-08-16). Each marker below is a
# distinctive user-facing serial string emitted by the feature; if the
# feature becomes unreachable, the linker drops the string with it, and
# this check fails the build instead of a bench session failing weeks
# later.
#
# Also enforces an OTA-slot SIZE MARGIN when handed an app image (see the
# size-margin section below).
#
# Usage: tools/image-check.sh <elf-or-app-image>
# Wired into: firmware/build-esp32.sh (every local build) and
# .github/workflows/release.yml (every release, all boards).
#
# When adding a feature whose silent absence would be invisible until a
# hardware session, add a marker here. Markers must be literal prefixes of
# println! strings (format-arg placeholders split the literal — use only
# the part before the first {}).
set -euo pipefail
IMG=${1:?usage: image-check.sh <elf-or-app-image>}

# "<literal marker>|<what its absence means>"
MARKERS=(
  "takeover: foreign partition table|WLED takeover (src/takeover.rs) is not linked — via-WLED installs would silently no-op"
  "provisioning AP|AP-mode provisioning is not linked — credless (release) images would be unreachable after flashing"
  "boot guard:|boot-loop guard is not linked — a bad OTA would wedge devices instead of self-healing"
)

# Feature-gated markers: asserted only when the caller declares the cargo
# feature was requested (EXPECT_FEATURES, space-separated — build-esp32.sh
# passes its feature list; release.yml passes the variant's extras).
if [[ " ${EXPECT_FEATURES:-} " == *" hub75 "* ]]; then
  MARKERS+=(
    "hub75: |the HUB75 panel driver (src/hub75.rs) is not linked — a hub75 build would boot with dead output"
  )
fi

fail=0
for m in "${MARKERS[@]}"; do
  s=${m%%|*}
  what=${m#*|}
  # grep -a: NixOS grep is ugrep and skips binaries without it
  if ! grep -aq -- "$s" "$IMG"; then
    echo "image-check: MISSING marker '$s'" >&2
    echo "             → $what" >&2
    fail=1
  fi
done

if [ "$fail" != 0 ]; then
  echo "image-check: $IMG is missing load-bearing features — a call site is" >&2
  echo "probably commented out or feature-gated off. See tools/image-check.sh." >&2
  exit 1
fi
echo "image-check: ok — all load-bearing features linked ($IMG)"

# ---------------------------------------------------------------------------
# OTA-slot size margin
#
# The app must fit the 1 MiB OTA slot (firmware/partitions.csv), but "fits"
# is too late a signal: /api/ota rejects an oversized image before writing,
# and the tightest board (board-c6-devkit, 43,872 B / 4.2 % left at v0.1.39)
# has no serial-recovery hardware on the bench (Gitea #56/#160). Two medium
# features ate ~7 KB of its headroom in two days.
#
# So: fail the build once the margin drops below MIN_MARGIN_PCT, and warn
# below WARN_MARGIN_PCT. 3 % (31,457 B) is the floor — it is under today's
# tightest board with ~12 KB of runway, so this does not red-light master
# on day one, while still stopping roughly two more features' worth of
# growth from reaching a device. 6 % is the warn line: every board except
# the C6 is well above it, so a warning means "a board just joined the C6
# in the danger zone".
#
# Only applies to app images (ESP image magic 0xE9). An ELF is not the
# thing that has to fit, so build-esp32.sh's ELF call skips this half.
# Overridable per-call: OTA_MAX / MIN_MARGIN_PCT / WARN_MARGIN_PCT.
# ---------------------------------------------------------------------------
OTA_MAX=${OTA_MAX:-1048576}          # 0x100000, firmware/partitions.csv
MIN_MARGIN_PCT=${MIN_MARGIN_PCT:-3}
WARN_MARGIN_PCT=${WARN_MARGIN_PCT:-6}

magic=$(od -An -tx1 -N1 "$IMG" | tr -d ' \n')
if [ "$magic" != "e9" ]; then
  echo "image-check: not an app image (magic 0x$magic) — skipping size-margin check"
  exit 0
fi

sz=$(stat -c%s "$IMG")
margin=$((OTA_MAX - sz))
# hundredths of a percent, integer-only (no bc/python on minimal runners);
# truncated, so the printed number never flatters a margin over a threshold
pct100=$((margin * 10000 / OTA_MAX))
pct=$(printf '%d.%02d' $((pct100 / 100)) $((pct100 % 100)))

if [ "$margin" -lt 0 ]; then
  echo "image-check: FAIL — $IMG ($sz B) EXCEEDS the $OTA_MAX B OTA slot by $((-margin)) B" >&2
  echo "             /api/ota would reject it. See docs/size-report.md for the diet list." >&2
  exit 1
fi

if [ $((margin * 100)) -lt $((OTA_MAX * MIN_MARGIN_PCT)) ]; then
  echo "image-check: FAIL — $IMG size $sz B, only $margin B ($pct %) of the" >&2
  echo "             $OTA_MAX B OTA slot left; the floor is $MIN_MARGIN_PCT %." >&2
  echo "             Shrink the image (docs/size-report.md) or, if this board is" >&2
  echo "             deliberately allowed to run tighter, raise MIN_MARGIN_PCT" >&2
  echo "             for it and say why. Gitea #160." >&2
  exit 1
fi

if [ $((margin * 100)) -lt $((OTA_MAX * WARN_MARGIN_PCT)) ]; then
  echo "image-check: WARNING — only $margin B ($pct %) of the OTA slot left" >&2
  echo "             (warn line $WARN_MARGIN_PCT %, hard floor $MIN_MARGIN_PCT %). Gitea #160." >&2
fi

echo "image-check: size ok — $sz B, $margin B ($pct %) of the $OTA_MAX B OTA slot free"
