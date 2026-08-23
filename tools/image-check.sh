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
