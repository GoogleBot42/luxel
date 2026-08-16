---
name: deploy-device
description: Use when pushing Luxel firmware and/or web assets to a device over the network (OTA push, asset-bundle push, or both) — the dev unit, the Athom rig, or any other reachable Luxel device.
---

This is a thin pointer to the real tooling plus the gotchas that have
actually bitten this project. Full per-script usage lives in each script's
own header and in docs/tools.md — read those before improvising a variant.

## 1. Pick the tool

- `tools/deploy.sh <device-ip> [--fw-only|--assets-only]` is the one-shot
  path. Run it from the repo root inside `nix develop`. With no flag it
  builds firmware (`firmware/build-esp32.sh $BOARD`, `BOARD` env var
  defaults to `board-pixelblaze-v3`), OTAs it via `tools/ota-push.sh`, then
  builds the web app, packs it into a LUXA archive
  (`web/tools/pack-assets.mjs`), and streams it to `POST /api/assets`.
  `--fw-only` skips the asset step; `--assets-only` skips the firmware
  build and OTA entirely (the flag you want after a serial recovery — see
  the gotcha below).
- `tools/ota-push.sh <ip> [image]` is the firmware-only half that
  `deploy.sh` calls internally. Use it directly when assets don't need
  touching.

## 2. Devices this skill covers

LAN IPs aren't secrets and are listed here for convenience; WiFi
credentials and any MQTT/HA broker details are not — those live in agent
memory (athom-flash-rig.md / jeremy-ha-broker.md memory files) and in
`firmware/creds.env` (gitignored).

- **Dev unit** — 192.168.0.205, DHCP hostname `luxel-4ae0d4`. Its power
  state varies day to day (it has been found unplugged before) — check
  reachability first, e.g. `curl -sm3 http://192.168.0.205/api/status`,
  before assuming a push failed for a code reason.
- **Athom rig** — 192.168.0.183, DHCP hostname `luxel-797e10`. Same board
  the athom-rig skill covers for serial/power recovery; deploy.sh and
  ota-push.sh are the normal (non-recovery) path to it, and unlike the dev
  unit it has remote power control (see the athom-rig skill) so its
  up/down state is more controllable.

**Autonomy**: OTA / live-coding / soak testing on both devices above is
pre-authorized per CLAUDE.md — no need to ask before pushing.

## 3. Gotchas

- **A credless image strands the device.** `tools/ota-push.sh` refuses to
  push an image that doesn't contain the baked WiFi SSID string (checked
  with `grep -a` against the binary). An image built without creds boots
  offline, and since OTA itself needs the network, that's a remote lockout
  requiring Jeremy's hands-on serial recovery. This happened for real
  twice (2026-07-05 and 2026-07-06, per UPDATES.md). Creds come from
  `firmware/creds.env` (gitignored); `firmware/build-esp32.sh` sources it
  automatically, so always build through that script — which `deploy.sh`
  does — rather than hand-rolling a build.
- **A reported OTA failure is often actually a success.** The device
  reboots into the new slot before the HTTP response finishes landing, so
  the `curl`/`ota-push.sh` exit code frequently reports failure on a push
  that worked (a known, still-open cosmetic flaw — see UPDATES.md's
  v0.1.32 entry). Don't trust the exit code alone: verify by polling
  `GET /api/status` and checking the `version` and `slot` fields (it also
  reports `fps`, `pixels`, `heap_free`, `live`, `src`, `bc`, `vmerr`).
  `ota-push.sh` already runs this polling loop itself and prints the
  post-reboot status line — read that line rather than just its exit code.
- **Serial flash leaves assets stale.** A serial `espflash flash` rewrites
  only the app partition; the assets partition keeps whatever it had
  before. Any serial recovery (see athom-rig skill) must be followed by
  `tools/deploy.sh <ip> --assets-only` before the web UI on that device can
  be trusted again.
- **`ota-push.sh` does NOT rebuild — it pushes the existing ELF.** After
  editing firmware sources (or switching branches / stashing), run
  `firmware/build-esp32.sh` first or you push a stale image with no
  warning; both images can even report the same version string
  (2026-08-15: an A/B test silently pushed the wrong build this way).
- **Stop a playing playlist before any flash-touching push — ONLY on
  v0.1.34–v0.1.35**: those builds' per-swap flash persist took the driver
  for the whole burst, so asset pushes failed ("flash write failed"),
  `/api/ota` rejected ("update already in progress"), and served assets
  truncated mid-body. `POST /api/playlist/stop`, push, then restore with
  `POST /api/playlist/play` (body = index). Fixed in v0.1.36 (borrow-per-op
  flash writes) — check `version` in `/api/status` first; on v0.1.36+
  pushes are safe with a playlist churning (verified 2026-08-15: 6/6 asset
  pushes, OTA accepted, no truncation, 5/5 cold loads).
- **After any crashy test run, re-check `slot`, not just `version`.** The
  boot-loop guard flips slots silently after 3 failed boots, and when both
  slots hold the same version the rollback is invisible in `version` —
  minutes of measurements were once taken against the rolled-back build
  (2026-08-15). Distinguish builds by `slot` (or a build-specific status
  field), and re-verify which slot is live before trusting any on-device
  measurement.
- **1 MiB OTA slot ceiling.** The app image must fit the OTA slot or
  `/api/ota` rejects it before writing. Per-board margins and how to
  reclaim space if a push starts failing for size live in docs/boards.md
  ("The 1 MiB OTA-slot ceiling").

## 4. After deploying

- Sanity check `GET /api/status`: confirm `version`/`slot` match what you
  just pushed and `vmerr` is null.
- For any change touching LED output (protocol, timing, buffer handling),
  run the hardware soak: `tools/hw-bench.mjs <ip> [report.md]` (see
  docs/tools.md) — it churns the full pattern gallery on the real device
  and writes `docs/bench-report.md`. Watch `firmware/serial.log` alongside
  it for panics if it's reachable; it's fed from outside the container and
  can go stale, so check its mtime before trusting what it shows.

## Failure modes

- **Push hangs or times out**: confirm the device is actually reachable
  first (`curl /api/status`) — the dev unit in particular is not always
  powered.
- **Device doesn't come back within `ota-push.sh`'s wait window**: don't
  assume it's fine. Check `firmware/serial.log` for a boot loop — three
  failed boots trips the firmware's boot-loop guard and rolls back to the
  other slot automatically, so a device that comes back on the OLD
  slot/version (not the one you just pushed) means the new image crashed
  on boot.
- **Athom device unreachable at all** (not just an OTA hiccup): escalate
  to the athom-rig skill for serial/power-level recovery.
