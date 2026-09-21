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

# Stash the caller's OTA_MAX before anything else: sourcing
# firmware/board-target.sh below and calling `board_ota_max` SETS OTA_MAX, so
# by the time the size section runs we can no longer tell an env override
# from a board default. The override is the escape hatch — it has to win.
OTA_MAX_ENV=${OTA_MAX:-}
BOARD_OTA_MAX=          # set by the EXPECT_FEATURES loop if a board is named
BOARD_OTA_MAX_FOR=

# "<literal marker>|<what its absence means>"
MARKERS=(
  "provisioning AP|AP-mode provisioning is not linked — credless (release) images would be unreachable after flashing"
  "boot guard:|boot-loop guard is not linked — a bad OTA would wedge devices instead of self-healing"
)
TAKEOVER_MARKER="takeover: foreign partition table"
MIGRATE_MARKER="migrate: partition table on flash"

# Board identity (Gitea #389). If EXPECT_FEATURES names a board, the image
# must contain THAT board's `board::NAME` string (main.rs prints it at
# boot; firmware/board-target.sh's `board_name` is the map). This catches
# two things at build time, before an image can reach a device: a build
# that silently used a different board than the command line suggests, and
# `board_name` drifting out of sync with firmware/src/board.rs. It is the
# same string tools/ota-push.sh checks at the push — the wrong board's
# image boots fine and differs only in RESERVED_PINS and pin defaults.
# The WLED takeover (src/takeover.rs, the `wled-takeover` feature) is per
# board — see `board_takeover` in firmware/board-target.sh. Assert BOTH
# directions: a WLED-capable board that lost the feature would ship an
# installer that silently no-ops, and a serial-only board that kept it would
# carry ~25 KB it can never use (Gitea #501). No board named → no assertion,
# which is how a bare `image-check.sh <elf>` on an unknown build still works.
ABSENT_MARKERS=()
for _f in ${EXPECT_FEATURES:-}; do
  case "$_f" in
    board-*)
      # shellcheck source=../firmware/board-target.sh
      . "$(dirname "$0")/../firmware/board-target.sh"
      if board_name "$_f"; then
        MARKERS+=("$BOARD_NAME|this image is not a $_f build — either the wrong board was built, or board_name in firmware/board-target.sh has drifted from board::NAME in firmware/src/board.rs")
      fi
      if board_takeover "$_f"; then
        if [ "$TAKEOVER" = 1 ]; then
          MARKERS+=("$TAKEOVER_MARKER|WLED takeover (src/takeover.rs) is not linked into a board that ships it — via-WLED installs would silently no-op. $_f must enable the wled-takeover feature in firmware/Cargo.toml")
        else
          ABSENT_MARKERS+=("$TAKEOVER_MARKER|WLED takeover (src/takeover.rs) is still linked into $_f, which is installed over serial — ~25 KB of OTA slot for code that can never run. Drop wled-takeover from its feature list in firmware/Cargo.toml")
        fi
      fi
      # …and the board's app-slot size, for the margin gate below. Slots are
      # per board since the #501 repartition (1.25 MiB on the 4 MB boards,
      # 3 MiB on the 16 MB Seengreat), and board-target.sh is where that
      # number lives — gating every board against the smallest one would be
      # a lie in both directions.
      if board_ota_max "$_f"; then
        BOARD_OTA_MAX=$OTA_MAX
        BOARD_OTA_MAX_FOR=$_f
      fi
      ;;
  esac
done

# The partition-layout migrator (src/migrate.rs, Gitea #501) is the //SIZETEST
# class all over again, with worse consequences: an image that silently lost it
# looks perfectly healthy and simply never moves the device off the pre-#501
# 1 MiB table — and the devices that need it have no serial console. So it is
# asserted on EVERY board, in both directions: present unless `migrate-off`
# (the deliberate retirement knob, firmware/Cargo.toml) is named, absent when
# it is. Retiring the migrator has to be a feature flag, never an edit.
if [[ " ${EXPECT_FEATURES:-} " == *" migrate-off "* ]]; then
  ABSENT_MARKERS+=(
    "$MIGRATE_MARKER|the layout migrator (src/migrate.rs) is still linked into a migrate-off image — the ~12 KB this mode exists to reclaim is still there"
  )
else
  MARKERS+=(
    "$MIGRATE_MARKER|the partition-layout migrator (src/migrate.rs) is not linked — this image would never move a device off the pre-#501 1 MiB partition table, and the devices that need it have no serial recovery (Gitea #501)"
  )
fi

# Feature-gated markers: asserted only when the caller declares the cargo
# feature was requested (EXPECT_FEATURES, space-separated — build-esp32.sh
# passes its feature list; release.yml passes the variant's extras).
if [[ " ${EXPECT_FEATURES:-} " == *" hub75 "* ]]; then
  MARKERS+=(
    "hub75: |the HUB75 panel driver (src/hub75.rs) is not linked — a hub75 build would boot with dead output"
  )
fi

# More markers that must be ABSENT. `hosted-ui` (Gitea #11) is a subtractive
# mode: the failure it can suffer is the opposite of //SIZETEST — the asset
# reader still being linked, so the image ships the very code the mode exists
# to remove and the measured saving quietly evaporates. Assert both
# directions. (ABSENT_MARKERS was opened above, with the takeover pair.)
if [[ " ${EXPECT_FEATURES:-} " != *" hosted-ui "* ]]; then
  # The cache-MMU mapping of the assets partition (src/flashmap.rs, Gitea
  # #259): its absence would silently put every asset response back on the
  # flash-controller path. hosted-ui images map nothing, hence the guard.
  MARKERS+=(
    "flashmap: assets |the assets flash mapping (src/assets.rs map_region) is not linked — assets would stream via flash-controller reads"
  )
fi
if [[ " ${EXPECT_FEATURES:-} " == *" hosted-ui "* ]]; then
  MARKERS+=(
    "assets: hosted-ui build|this is not a hosted-ui image (src/assets.rs init) — the wrong feature set was built"
  )
  ABSENT_MARKERS+=(
    "assets: none installed|the LUXA reader (src/assets.rs init) is still linked into a hosted-ui image"
    "not a LUXA archive|the asset installer (src/assets.rs AssetWriter) is still linked into a hosted-ui image"
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

for m in ${ABSENT_MARKERS+"${ABSENT_MARKERS[@]}"}; do
  s=${m%%|*}
  what=${m#*|}
  if grep -aq -- "$s" "$IMG"; then
    echo "image-check: UNEXPECTED marker '$s'" >&2
    echo "             → $what" >&2
    fail=1
  fi
done

if [ "$fail" != 0 ]; then
  echo "image-check: $IMG does not match the requested feature set — a call" >&2
  echo "site is probably commented out, or gated on/off wrongly. See" >&2
  echo "tools/image-check.sh." >&2
  exit 1
fi
echo "image-check: ok — all load-bearing features linked ($IMG)"

# ---------------------------------------------------------------------------
# OTA-slot size margin
#
# The app must fit its OTA slot (firmware/partitions.csv — or
# partitions-16mb.csv on the one 16 MB board), but "fits" is too late a
# signal: /api/ota rejects an oversized image before writing, and the
# tightest board (board-c6-devkit, 43,872 B / 4.2 % left at v0.1.39) has no
# serial-recovery hardware on the bench (Gitea #56/#160). Two medium features
# ate ~7 KB of its headroom in two days.
#
# So: fail the build once the margin drops below MIN_MARGIN_PCT, and warn
# below WARN_MARGIN_PCT. 3 % is the floor — it was set under the then
# tightest board with ~12 KB of runway, so it does not red-light master,
# while still stopping roughly two more features' worth of growth from
# reaching a device. 6 % is the warn line: a warning means "a board just
# joined the C6 in the danger zone".
#
# WHICH SLOT (Gitea #501). Slots stopped being one number when the tables
# were repartitioned, so resolve in this order and SAY which rule won:
#   1. OTA_MAX in the environment — the unchanged per-call escape hatch.
#   2. MIGRATING_RELEASE=1 — see below.
#   3. EXPECT_FEATURES names a board-* → `board_ota_max` from
#      firmware/board-target.sh (1,310,720 on the 4 MB boards, 3,145,728 on
#      board-seengreat-hub75). One source of truth per board.
#   4. nothing known about the image → 1,048,576, the pre-#501 slot. The
#      conservative answer for a bare `image-check.sh <elf>` on some
#      unidentified build.
#
# MIGRATING_RELEASE=1 is the transition switch, and it is a FLOOR, not a
# ceiling: during the release that carries devices from the old table to the
# new one, the image has to be installed by a device that has NOT
# repartitioned yet — it is written into a 1 MiB slot by the running old
# firmware. So the gate weighs it against 1,048,576 whatever the board's new
# slot says. The margin floor drops to **0 %** for that one release — "it
# fits" is the whole requirement — because the repartition is precisely what
# ends the squeeze, and holding any floor against the OLD slot would block
# the release that makes the slot bigger. That is not a comfortable number
# and it is not meant to be: measured 2026-09-20 the migrating images leave
# 2,992 B (0.28 %) on board-c6-devkit and 8,016 B (0.76 %) on
# board-athom-music, and three of the nine were ALREADY under the 3 % floor
# on master before #501 added a byte. The same images land at 20-25 % of
# their new slots the moment the device reboots into the new table, which is
# the entire point. REMOVE the switch in the next release — the floor goes
# back to 3 % of the per-board slot and there is finally room under it.
# Tracked as Gitea #635; see docs/releases.md and
# .github/workflows/release.yml.
#
# Only applies to app images (ESP image magic 0xE9). An ELF is not the
# thing that has to fit, so build-esp32.sh's ELF call skips this half.
# Overridable per-call: OTA_MAX / MIN_MARGIN_PCT / WARN_MARGIN_PCT.
# ---------------------------------------------------------------------------
MIGRATING=${MIGRATING_RELEASE:-0}
MIGRATING_NOTE=
if [ -n "$OTA_MAX_ENV" ]; then
  OTA_MAX=$OTA_MAX_ENV
  OTA_MAX_RULE="OTA_MAX=$OTA_MAX_ENV from the environment (explicit override)"
elif [ "$MIGRATING" = 1 ]; then
  OTA_MAX=1048576
  OTA_MAX_RULE="MIGRATING_RELEASE=1 — the OLD 1 MiB slot, not this board's new one"
elif [ -n "$BOARD_OTA_MAX" ]; then
  OTA_MAX=$BOARD_OTA_MAX
  OTA_MAX_RULE="board_ota_max $BOARD_OTA_MAX_FOR (firmware/board-target.sh)"
else
  OTA_MAX=1048576
  OTA_MAX_RULE="default — no board in EXPECT_FEATURES, assuming the pre-#501 1 MiB slot"
fi
if [ "$MIGRATING" = 1 ]; then
  # 0 rather than 3: see the MIGRATING_RELEASE paragraph above — for this
  # one release the gate is "does it fit the old slot at all". An explicit
  # MIN_MARGIN_PCT still wins, same as OTA_MAX does.
  MIN_MARGIN_PCT=${MIN_MARGIN_PCT:-0}
  MIGRATING_NOTE="MIGRATING RELEASE: measured against the OLD 1 MiB slot — the image must still fit a device that has not repartitioned yet (Gitea #501)"
else
  MIN_MARGIN_PCT=${MIN_MARGIN_PCT:-3}
fi
WARN_MARGIN_PCT=${WARN_MARGIN_PCT:-6}
echo "image-check: OTA slot $OTA_MAX B, floor $MIN_MARGIN_PCT % — $OTA_MAX_RULE"
if [ -n "$MIGRATING_NOTE" ]; then
  echo "image-check: *** $MIGRATING_NOTE ***"
fi

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

# Every size line names the slot AND the rule that chose it — a margin
# number is meaningless without knowing which slot it is a fraction of, and
# during the migrating release it is a fraction of the OLD one.
SLOT="$OTA_MAX B OTA slot [$OTA_MAX_RULE]"

if [ "$margin" -lt 0 ]; then
  echo "image-check: FAIL — $IMG ($sz B) EXCEEDS the $SLOT by $((-margin)) B" >&2
  [ -n "$MIGRATING_NOTE" ] && echo "             $MIGRATING_NOTE" >&2
  echo "             /api/ota would reject it. See docs/size-report.md for the diet list." >&2
  exit 1
fi

if [ $((margin * 100)) -lt $((OTA_MAX * MIN_MARGIN_PCT)) ]; then
  echo "image-check: FAIL — $IMG size $sz B, only $margin B ($pct %) of the" >&2
  echo "             $SLOT left; the floor is $MIN_MARGIN_PCT %." >&2
  [ -n "$MIGRATING_NOTE" ] && echo "             $MIGRATING_NOTE" >&2
  echo "             Shrink the image (docs/size-report.md) or, if this board is" >&2
  echo "             deliberately allowed to run tighter, raise MIN_MARGIN_PCT" >&2
  echo "             for it and say why. Gitea #160." >&2
  exit 1
fi

if [ $((margin * 100)) -lt $((OTA_MAX * WARN_MARGIN_PCT)) ]; then
  echo "image-check: WARNING — only $margin B ($pct %) of the $SLOT left" >&2
  [ -n "$MIGRATING_NOTE" ] && echo "             $MIGRATING_NOTE" >&2
  echo "             (warn line $WARN_MARGIN_PCT %, hard floor $MIN_MARGIN_PCT %). Gitea #160." >&2
fi

echo "image-check: size ok — $sz B, $margin B ($pct %) of the $SLOT free"
if [ -n "$MIGRATING_NOTE" ]; then
  echo "image-check: $MIGRATING_NOTE"
fi
