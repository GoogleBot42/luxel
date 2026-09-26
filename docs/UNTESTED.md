# Things Jeremy hasn't personally tried yet

Everything since your last hands-on session (the night of 2026-07-06, when
you verified the clean device load + the "untitled pattern" fix). All of it
is machine-verified (unit tests, three e2e suites, and where possible the
real device/broker) — but "works in a test" and "feels right" are different
things. Ordered roughly by how much they benefit from your eyes. Check off
as you go; tell me anything that feels wrong and it becomes a task.

Everything runs on the wall unit at http://192.168.0.205/ unless noted.

## Quick wins (open the web UI and click around)

- [ ] **Playlist** (v0.1.15–17): save a few patterns to the device, add
  them to the Playlist tab (same pattern twice with different slider values
  works), set a default duration + a per-item override, hit play. Reboot the
  device mid-playlist — it should resume where it was.
- [ ] **Crossfade** (v0.1.17): Playlist tab → "crossfade" field (e.g. 2
  seconds) — items should blend into each other on the wall, not snap.
- [ ] **Gallery search + 195 tiles** — the Patterns Library search box; the
  five new **render3D patterns render as rotating point clouds** (search
  "3D").
- [ ] **Editor DX** (your work-day picks): Ctrl/Cmd+S saves, playlist rows
  drag to reorder.
- [ ] **Sound-reactive playground**: open any pattern, add
  `export var energyAverage` and use it in `render`, hit the **sound**
  button next to *debug*, allow the mic — the preview should pump with your
  voice. In device mode the same button ALSO drives the wall
  (mic → strip at ~20 Hz).
- [ ] **Event injection** (v0.1.38/39): open *Typing Heatmap 2D*,
  *Crosshair Pulse 2D*, *Ripples 2D* or *Slime mold palette* and click/drag
  the preview — hits should land where
  you click (and on the wall too, in device mode; *SaberDeploy Tutorial*
  takes any event as a button press). Then from HA (or
  `mosquitto_pub` at your broker): publish `1 0.5 0.5` to
  `luxel/luxel-4ae0d4/event` while one of those patterns runs — the wall
  should react. Machine-verified against local mosquitto via the mirror;
  the real-HA hop is the untested bit.
  (Firmware HTTP path hardware-soaked 2026-08-22 on the Athom —
  tools/event-soak.mjs, 30,286/30,286 delivered, malformed rejected,
  heap/fps stable. Still open here: the preview click-through and the
  real-HA MQTT hop on the wall unit — both tracked as Gitea #216.)
- [x] **Share links carry the pattern only** (changed by Gitea #463): a map is
  the Layout's, not the pattern's, so a link made today is `#p=` even with a
  custom map installed, and an old `#pj=` link still restores its map as the
  playground's Layout choice. Both machine-verified in `web/tools/e2e.mjs`.
- [ ] **3D map preview**: a map program whose z varies (e.g.
  `plot(cos(a), sin(a), index/pixelCount - 0.5)`) → auto-rotating point
  cloud with a "3D" badge, on the map program's own screen AND in the editor
  preview once the map is in use. (The screen is reached from the playground's
  "Preview as → Custom map program" since A10/#471.)

## Settings tab (device mode)

The whole page was reshaped by A8 (Gitea #469, v2): Device (brightness first)
· LED layout · WiFi, then one **Advanced** list whose rows each state their
value while collapsed. Everything below is on the new page.

- [ ] **The ranked page itself** — does the order match what you actually open
  Settings for? Brightness is the first control; the Advanced rows should be
  scannable without expanding any of them.
- [ ] **LED layout on the Athom** — the summary line, the pixel count (it
  resizes live, no reboot), LED type / colour order / data pin. The Athom
  advertises two outputs, so the **Outputs table** should be there with an
  `+ Add output`: adding one splits the strip in half and says
  `pixels 0–29 / 30–59`. The table is built at boot (#474), so the page says
  "applies after a reboot" and offers a `Reboot to apply` beside it — with a
  second strip on GPIO 19 the back half of the fixture should light after that.
- [ ] **LED layout on the HUB75 panel** — there should be NO layout picker at
  all (the board has no choice) and no LED type / colour order / data pin;
  the panel size, `Panels [c] across × [r] down` and a collapsed
  `Panel module` row instead (the scan rate moved inside it, #778). Setting
  2×2 should draw the chain picture and drop the estimated refresh to ~28 Hz
  in amber, and — because a 64×64 board only shifts 64 columns of chain — say
  that three of the four panels would stay dark. **Do not apply it** unless you
  actually have four panels: the `Reboot to apply` button beside the note is
  what builds it, and it reboots.
- [ ] **The estimated refresh number** — one 64×64 panel should read 115 Hz,
  which is what the panel measures (`rescan_hz`, shown beside it as
  "measured now"). It is computed from the driver the device reports as
  CONFIGURED (LED layout › Panel module), so it follows that form; a
  disagreement with the measurement means the firmware and the panel
  disagree, not the browser.
- [ ] **LED layout › Panel module on the Seengreat** (Gitea #401/#525/#771/#778)
  — a collapsed row inside the LED layout section now, not in Advanced (#778).
  Its one line should read the whole module
  (`1/32 scan · plain shift register · 30 MHz · 7 planes · blanking 1`), and
  opening it should give: **Scan rate** (`1/32 (usual for 64 rows)`, `1/16`,
  `1/8`, `1/4` — never "board default"), driver chip, pixel clock, bit planes,
  latch blanking. The pixel clock is a **dropdown of 8/10/12/15/20/24/30 MHz**,
  not a number field (#771 — 40 MHz is no longer reachable at all, and nothing
  above 30 is offered). Pick **20 MHz** and reboot (the row says "reboot to
  apply" until you do, and the card names what is still running): `rescan_hz`
  should come back at **~77 Hz**, and the estimate beside it should already have
  said so. Two reboots inside a minute should be harmless: before #771 that
  tripped the boot-loop guard and rolled the firmware back.
- [ ] **Latch blanking is LIVE — the one to watch the panel for** (Gitea #778).
  With the SM16208SF tiles running, raise **latch blanking** from 1 to 2, then
  3, then 4, **without rebooting**, and watch the panel as each one lands: the
  ghosting between address rows should visibly change within a frame, and the
  picture should get slightly dimmer as the OE window narrows. Nothing should
  raise the reboot bar, `reboot_required` should be `false` in the reply, and
  `driver.live.blank` should follow within a frame (`GET /api/layout`). This is
  the check the whole change exists for — "let's make latch blanking dynamic, I
  see value in that one" — and it is the one thing no harness can see: a control
  bit change is invisible to every counter the device has and byte-identical in
  the composed frame. Also worth trying at the far end: **blanking 8** on the
  64-column panel is legal and should simply be very dim, while the device
  should REFUSE a blanking that leaves no lit clock at all, naming the numbers
  (`panel: blank N + 1 latch clocks leave no lit clock in a 64-word row block` —
  reachable only on a much narrower row block than this panel's).
- [ ] **Firmware & recovery → Update…** — pick a `luxel.bin` for that board
  and let it flash itself over the network. **Never run against hardware**
  (the mirror advertises no OTA, so no harness can reach this path) —
  Gitea #526 has the full procedure and what to read before and after.
- [ ] **WiFi form** — now collapsed behind `Change network…`; shows the saved
  network, and changing creds reboots onto the new one (careful: typos strand
  it → AP mode should catch it now).
- [ ] **Device map upload** (v0.1.16): **Install on device**, the single
  primary action of the map program's own screen (A10/#471 — reached from
  Settings → LED layout → "Custom map program →"); the wall renders with real
  geometry and it survives reboot. Note that nothing hands the PROGRAM back
  from a device yet (Gitea #517) — only its coordinates — so check the program
  text is the one this browser wrote.
- [ ] **Network input status row** — while LedFx/xLights (or my test
  script) streams DDP, the collapsed Advanced row says "receiving DDP" and the
  pattern resumes a few seconds after the stream stops.
- [ ] **Multi-device sync role select** — with one device it just shows
  "broadcasting"/"waiting"; the real test needs a second Luxel someday.
- [ ] **MQTT form** — your broker is already configured and connected
  (I set it up); the row should say "connected".

## Home Assistant (check your HA UI)

- [ ] Device **luxel-4ae0d4** should exist with: a **Light** (power +
  brightness — off blanks the strip, on resumes mid-animation), a
  **Pattern select** (device library by name), and — after the next OTA
  (v0.1.23, pending) — **FPS/heap diagnostic sensors**, a **Playlist
  switch**, and **Next/Previous pattern buttons**.
- [ ] Power/brightness from HA and from the web UI stay in agreement
  (state echoes within ~5 s either way).

## Needs you physically (I can't do these)

- [x] **The partition repartition on metal — the 4 MB half** (Gitea #634,
  from #501): done on the **Athom** 2026-09-20. One reboot, under 8.6 s end
  to end, patterns / playlist / layout / name / brightness / asset bundle all
  intact, `partitions.migrated true`, slot 1,310,720 B, store 524,288 B.
  Timeline and the full before/after tables are on #634; the fleet status
  table is in docs/boards.md ("On metal"). Two things came out of it: the
  resume-after-a-cut path got **no** coverage (the migration finished before
  the cut window could open — Gitea #644), and an OTA across an LXBC format
  bump leaves every stored blob unreadable on a device that cannot recompile
  itself (Gitea #643).
- [ ] **The partition repartition on metal — the Seengreat** (Gitea #634):
  **attempted 2026-09-21; it declined, and the cause is now known.** Its
  bootloader was serially flashed when this board still used
  `partitions.csv`, so it tells the ROM the part is 4 MB on 16 MB of
  silicon — and the ROM bounds-checks every flash op against that, so the
  new `storage` erase at `0x610000` failed on its first sector. Found under
  emulation, not on the bench: the 16 MB image now runs on QEMU's `esp32s3`
  machine and the whole migration passes there from either slot, cut at
  every stage, with the asset bundle byte-identical at `0xa10000`
  (`tools/qemu/run-all.py -k migrate-s3`).
  **And it no longer refuses.** As of #634's second half a 16 MB image
  embeds the 4 MB table as well and takes the largest layout that fits under
  the bootloader ceiling, so the panel is expected to migrate to
  `partitions.csv` — 1.25 MiB slots, a 512 KiB store, `assets` left at
  `0x310000` — and report `upgrade_available:true`. **What is untested on
  metal is that fallback**, both hops of it, and the panel is the only board
  that can test either. Emulated end to end (`migrate-s3-fallback-*`,
  `migrate-s3-fallback-then-16mb`).
  Getting the panel onto its own 16 MB table still needs a **one-time serial
  flash of the bootloader** — Jeremy's hands and the panel's USB port —
  `BOARD=board-seengreat-hub75 firmware/build-esp32.sh flash`, which writes
  bootloader + table + app with `--flash-size 16mb`. If that happens before
  the panel ever takes the fallback it lands on the new table directly and
  the *self-applied* 16 MB path stays emulator-only until there is a second
  16 MB board; if it happens after, the device migrates a second time and
  that hop gets its first metal run. The residual risk on any such device is
  unchanged: a cut inside the single 4 KiB table write, milliseconds,
  serial-recovery-only.
  **The panel is currently being recovered over serial**: the diagnostic
  OTA of 2026-09-21 was written over the slot it was running from and the
  board boot-loops on the torn image (Gitea #655 — root cause, fix and the
  on-metal follow-up are there; #634 has the timeline). Note the fix could
  not have saved the panel's `ota_0` in place — there was no second image
  on the board once staging began — but it means the update would have been
  refused-or-safe rather than fatal.
- [ ] **`/api/ota` on metal after #655** (Gitea #668): the slot selection,
  head-last write and verify-before-activate have host tests and the boot
  line is asserted under QEMU, but no emulated guest can take an OTA. On
  the Athom: one OTA each way (`slot` alternates, `ota: updates go to …`
  on the boot log names the other slot), then a deliberately truncated
  upload (`head -c 500000 luxel-fw-ota.bin | curl --data-binary @-` with
  the real `Content-Length`) must answer an error, leave `slot` unchanged
  across a reboot, and the next full OTA must land.
- [ ] **AP-mode provisioning** (v0.1.22): Settings → "reboot into setup
  AP", then join `luxel-4ae0d4` from your phone — a captive portal should
  pop with the settings page; save WiFi and it reboots back onto your
  network. One-shot: if anything goes wrong, power-cycle and it boots
  normally. **Nobody has tested the radio path.**
- [ ] **Serial flash flow** (next time you flash): `./build-esp32.sh flash`
  now also writes the web assets — after flashing, the served UI should be
  current with no extra step. Also `./build-esp32.sh image` → single
  full-flash restore file.
- [ ] **Onboard mic bring-up** — a bench session: see
  [docs/mic-bringup.md](mic-bringup.md). The FFT pipeline is ready; we
  need the mic's type + pins probed.
- [ ] **PB sensor board** (only if you own one): plugs into the expansion
  header RX0; sound-reactive patterns should react with zero config.
- [ ] **Strip on a moved data pin** (Gitea #238, #154): Settings → Device →
  "Data pin", pick a free output pin (Athom: GPIO33), apply & reboot, move
  the strip's DATA wire there — it should light; pick the board default to
  put it back. Headless-verified: the setting persists across the reboot and
  `/api/config` reports the bound pin; nobody has seen pixels on the new pin.
- [ ] **A real button through `digitalRead`** (Gitea #238, #177): the pattern
  in #238 on the Athom's case button (GPIO0) — dim strip, bright while held.
  Headless-verified up to the press: the pull-up idles the pad HIGH, and a
  written level reads back on the same pad.
- [ ] **Map-aware 2D blur/glow on a real matrix** (Gitea #448; kernels #140,
  panel #75): on the 64×64 HUB75 panel with its grid map installed, run a
  pattern with a bright point, then raise Settings → Output → Blur and
  Glow. It should soften into a round halo in both axes with no smear along
  the wiring and no bright/dark seam at the row folds. **Machine-verified on
  the panel 2026-09-08** (firmware `f62a45e`/v0.1.40, 4096 px, a real
  `kind: "grid"` 64x64 map — the old #258 blocker is gone): a one-pixel
  probe read back through `GET /api/pixels` gives the separable 1-2-1 kernel
  in *both* axes for `setBlur(0.5,1)`, a 5x5 disc for two passes, and a 3x3
  max-bloom halo with the source undimmed for `setGlow(0.6)`; a point at the
  last pixel of a row (r31c63) spreads to six edge-clamped cells and puts
  **nothing** at r32c0, its index neighbour — so no fold along the wiring.
  `vmerr` null, fps/heap healthy, `/api/output` round-trips (numbers and
  costs: docs/bulk-render.md “Map-aware 2D blur/glow on the 64x64 panel”,
  Gitea #446). What is left is the part a readback cannot do: **Jeremy
  looking at the panel**.
- [ ] **The hosted console reaching a device over https** (Gitea #162,
  needs a HEADFUL browser — no agent can do this one): open
  `https://googlebot42.github.io/luxel/?device=http://<device-host>` in
  normal Chrome on the LAN. Expect a Local Network Access permission
  prompt; allow it and the console should connect exactly as it does when
  served from the device. Deny it (or dismiss it) and the page should say
  the browser blocked it and offer the manual routes — that half is
  machine-verified (`web/tools/lna-e2e.mjs`), the granted half is not.
- [x] **WLED→Luxel installer page against real WLED** — DONE 2026-08-30:
  full stock-WLED-restore → Improv provision → credless-master takeover →
  boot-guard-healthy cycle on the Athom (Gitea #53 closed, PR #171, beta
  banner dropped). Only the https-origin / Chrome local-network-permission
  path remains, tracked as Gitea #162 (hardware-confirmed blocked in
  headless chromium; needs a headful browser).

- [ ] **One-file release install and the store self-heal** (Gitea #643) — the
  whole flow has never run on metal. On a real board: Settings → Advanced →
  Firmware & recovery → **Update…**, pick that board's
  `luxel-<board>-<ver>.luxr`, and watch it write the firmware, wait out the
  reboot, install the web app and reload itself. Things to confirm that a
  mirror cannot: that the 60 s boot window is actually enough over WiFi, that
  the browser survives the device going away mid-flow, and that an image for
  ANOTHER board is refused by name before anything is written. Then the
  repair half, which is only reproducible across a real LXBC format bump:
  after such an OTA the console should say "N stored patterns were compiled
  for an older engine — recompiling…", come back with the playlist playing,
  and leave every pattern id (and therefore every playlist entry) untouched.
  Machine-verified end to end against `luxel serve --accept-ota` /
  `--stale-store` / `--bc-format` (`web/tools/device-e2e.mjs`, 24 checks) and
  in unit tests; the hardware run is tracked as Gitea #526.

## Verified hard by machines, low review value (FYI only)

- Boot-loop guard (bad OTA self-heals by flipping slots) — exercised in
  anger during the v0.1.19 wedge; recovery notes in UPDATES.md.
- DDP/E1.31 pixel input — byte-verified on the wall over real WiFi.
- Oracle findings: rendering is now bit-exact vs your Pixelblaze
  (quantization + palette fixes); transforms/pow/log2 verified.
- Firmware size diet (91→87% of the OTA slot), deploy script
  (`tools/deploy.sh <ip>`), hardware bench report
  ([docs/bench-report.md](bench-report.md), regenerating as I write).
