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
memory (athom-flash-rig.md), `~/.config/mqtt/broker.env` (user-level
`power-switch` skill; jeremy-ha-broker.md memory has the care rules) and in
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
- **Seengreat HUB75 S3 + 64x64 panel** — 192.168.0.238, DHCP hostname
  `luxel-f6b0a8`, 64x64 matrix at 4096 px. Push it with
  `BOARD=board-seengreat-hub75 tools/ota-push.sh 192.168.0.238` — the script
  takes the ELF path AND the espflash `--chip` from `$BOARD` (since
  2026-09-06; the older advice to pass a hand-made image explicitly is
  obsolete). Leave it exactly as you FOUND it — brightness is Jeremy's
  setting (it was 3 on 2026-09-07), never a number restored from this file.
  Its OTA used to wedge ~44 % of pushes (#294) — #309's flash-fence fix cured
  that (13/13 clean in one session, 2026-09-06), so a failed push there is
  now a real failure, not the known flake.
  Its USB is the S3's native USB-Serial/JTAG at `/dev/ttyACM0`, and a plain
  open is **not** a reliable reset (it is often passive): the recipe that
  works is one long-lived `socat -u /dev/ttyACM0,raw,echo=0,b115200 STDOUT`
  reader plus a SECOND short socat open, which resets the chip while the
  first reader captures the whole boot log. `doas chmod 666 /dev/ttyACM0`
  first if the node came back 660. tools/hw-bench.mjs takes the short open as
  `HW_BENCH_RESET_CMD`. **Never touch serial in the 60 s after an OTA
  reboot** — the reset lands inside the pending-verify window and the
  bootloader rolls the new slot back. Physical EN/BOOT presses are Jeremy's.

**Autonomy**: OTA / live-coding / soak testing on the devices above is
pre-authorized per CLAUDE.md — no need to ask before pushing.

## 3. Gotchas

- **Found state is READ, never carried as a target.** A brief that names a
  brightness / pattern / pixel count invites an agent to "restore" the
  number in the brief — one restored brightness 31 over Jeremy's 3 that way
  (2026-09-07). Capture `GET /api/brightness`, `/api/pattern`,
  `/api/pattern.lxp` and `/api/config` first, and put back exactly what you
  read.
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
- **Set `BOARD` for anything but the classic ESP32.** `deploy.sh` and
  `ota-push.sh` both read it (through `firmware/board-target.sh`) to find
  the ELF and pick `espflash --chip`; without it they look for a
  `xtensa-esp32-none-elf` build that an S3/C3/C6 build never produced.
  `BOARD=board-seengreat-hub75 tools/deploy.sh 192.168.0.238`.
- **On a board whose OTA is flaky, push the same image to BOTH slots before
  measuring.** The Seengreat panel wedges ~44 % of OTAs (#294) and the boot
  guard/bootloader can roll a slot back at any time; with both slots
  carrying the same build, a rollback cannot silently move a benchmark onto
  a different image. Read `core1.last` in `/api/status` after a long
  session — on dual-core boards a watchdog reset leaves no other trace.
- **`ota-push.sh` can fail SILENTLY.** Its `curl -sf` to `/api/ota` exits
  non-zero on a non-2xx and `set -e` ends the script before the status
  poll, so the output simply stops after `pushing N bytes…` with no error.
  Both 2026-09-07 occurrences coincided with a background `/api/status`
  poll loop — don't poll during a push; a plain
  `curl --data-binary @app.bin http://<ip>/api/ota` of the same image
  worked. (The script now prints curl's exit status and the response body
  on failure and exits non-zero loudly.)
- **`firmware/build-esp32.sh <board>` does NOT take a board.** The
  positional is the ACTION (`flash`, `image`, `log`); the board comes from
  `$BOARD`. It used to quietly build board-pixelblaze-v3 and push that to
  the Athom (Gitea #389, 2026-09-07); since PR #416 a positional
  `board-*` is a hard error, `tools/image-check.sh` asserts the board's
  `board::NAME` at build time, and `tools/ota-push.sh` REQUIRES `BOARD=`
  and refuses an image that isn't that board's build, naming the board it
  looks like (`REFUSING to push: image is not a <board> build … looks like
  a <other> build`). That message means the build used the wrong board —
  rebuild with `BOARD=`, don't reach for `SKIP_BOARD_CHECK=1`. A plain
  `curl --data-binary @app.bin http://<ip>/api/ota` bypasses all of it.
- **`tools/stack-check.sh` overwrites the ELF `build-esp32.sh` wrote** with a
  `-Z emit-stack-sizes` build — same size, different bytes — so an image
  hashed or pushed after a stack-check run is not the shipping build.
  Build → save/hash the image → THEN stack-check (2026-09-07).
- **Never run two `build-esp32.sh` sweeps in parallel**, different
  worktrees included: they share `/tmp/img-*.bin` and the per-chip ELF
  path, so size/image results interleave and are garbage. Size runs are
  serial (2026-09-07).
- **`ota-push.sh` does NOT rebuild — it pushes the existing ELF.** After
  editing firmware sources (or switching branches / stashing), run
  `firmware/build-esp32.sh` first or you push a stale image with no
  warning; both images can even report the same version string
  (2026-08-15: an A/B test silently pushed the wrong build this way).
  Worse, **every board on the same chip shares that ELF path**
  (`firmware/target/xtensa-esp32-none-elf/release/luxel-fw` is
  board-pixelblaze-v3's, board-athom-music's AND board-esp32-generic's), so
  `BOARD=…` on the push does not protect you: anything that built a
  different board in between — `tools/ci.sh` (which builds `CI_BOARD`,
  default board-pixelblaze-v3), an image-size probe, another session's
  build in the same worktree — leaves ITS image there and you OTA the wrong
  board's firmware onto the device. It boots and serves HTTP, so nothing
  errors; the strip just goes dark on the wrong pin. Tells in
  `GET /api/config` / `/api/status`: a `data_pin_default` that isn't the
  board's (23 = pixelblaze-v3, 18 = athom), or board-conditional status
  fields missing. Rebuild the board you mean **immediately** before
  pushing, and check `data_pin_default` after (2026-09-06, cost a bad
  flash on the Athom).
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
- **Kill every attached serial reader BEFORE an OTA push.** The USB node
  re-enumerates on the reboot and the host re-applies termios, which on the
  S3's native USB-Serial/JTAG *is* a reset — so an attached reader makes
  every OTA two boots, and a third inside the `boot_ok` window flips the
  slot. Re-attach only after boot_ok (~75 s). (2026-09-07; the older "a
  reader that is already attached is harmless" reading is wrong.)
- **Every OTA reboot drops the live (ad-hoc) pattern back to the persisted
  default** — on the panel that is Rainbow, 52 fps at 4096 px, which reads
  exactly like a throughput regression against your last measurement. `GET
  /api/pattern` before believing any post-OTA number, and re-push the live
  pattern (2026-09-07).
- **There is no `/api/reboot`.** On strip boards `POST /api/datapin` with
  the CURRENT pin is a clean remote reboot. Two in quick succession trip
  the boot-loop guard (silent slot rollback), and the device answers for a
  moment after the request — detect a reboot by waiting for it to go DOWN
  first (2026-09-07).
- **`cargo` does not refingerprint a swapped `firmware/vendor/esp-hub75`.**
  The symlink points into the nix store where every mtime is 1970, so an
  A/B between two esp-hub75 patch versions silently reuses the stale rlib —
  `cargo clean -p esp-hub75` between them (2026-09-07).
- **`pkill -f <pattern>` kills your own shell** whenever the pattern also
  appears in the compound command you are running (exit 144) — this is
  general, not a ttyUSB0 quirk. Anchor it (`pkill -f '^socat -u
  /dev/ttyACM0'`) or bracket a character (`419[3]`), and give it its own
  call.
- **1 MiB OTA slot ceiling.** The app image must fit the OTA slot or
  `/api/ota` rejects it before writing. Per-board margins and how to
  reclaim space if a push starts failing for size live in docs/boards.md
  ("The 1 MiB OTA-slot ceiling").

## 4. After deploying

- Sanity check `GET /api/status`: confirm `version`/`slot` match what you
  just pushed and `vmerr` is null.
- Exercising a settings POST as a smoke test? GET the current state FIRST
  and restore it after — multi-field bodies make it easy to clobber a
  field you weren't testing (`POST /api/output` with a guessed `capMa 0`
  silently wiped the Athom's 850 mA cap, 2026-08-30; caught only because
  a pre-deploy response capture existed to diff against).
- For any change touching LED output (protocol, timing, buffer handling),
  run the hardware soak: `tools/hw-bench.mjs <ip> [report.md]` (see
  docs/tools.md) — it churns the full pattern gallery on the real device
  and writes `docs/bench-report.md`. It "restores" the device to **300 px**
  (its own header says so), NOT the count it found — on the Athom rig
  (as-found state: 60 px) follow with `POST /api/config` body `60`.
- **`tools/patbench.mjs` / `tools/opbench.mjs` DESTROY the live pattern** —
  they "restore" rainbow, and there is no read-back of an ad-hoc pattern.
  Capture the found `fps`/`vm_us`/`heap_free` AND the source (`GET
  /api/pattern.lxp`) before the first push. Don't fingerprint with a
  STATEFUL pattern (Infinite Snake v2): its numbers stop being comparable
  after the first minute (2026-09-07).
- **`/api/pixels` is the pipeline buffer UPSTREAM of the DMA.** It answers
  engine/compose questions and says nothing about display-side artefacts
  (tearing, skips, repeats) — for those read `/api/status`
  `pass`/`swap`/`dropped`/`rescan_hz` (post-#397/#398) or film the panel.
- For a change touching the HTTP response path (`firmware/src/server.rs`),
  run `tools/wire-check.sh <ip>` after the OTA (bare IP or `http://ip`, both
  work; exit 2 = HARNESS BROKEN and exit 3 = device unreachable, neither of
  which is a firmware verdict) — curl-level contract check
  of every response family (single Content-Type, Content-Length == body
  bytes incl. streamed routes, asset 200/304 caching headers, all four
  OPTIONS preflight headers, 404 shape). Watch `firmware/serial.log` alongside
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
